import React, { useState, useEffect, useRef } from 'react';
import { X, BarChart2, Play } from 'lucide-react';
import { FieldWells } from './FieldWells';
import { barSpan, signedScale } from '../utils/chartScale';
import { api } from '../api';
import { ChartType, ChartDefinition, ChartData, WorksheetTable } from '../types';

interface ChartModalProps {
  isOpen: boolean;
  onClose: () => void;
  tables: WorksheetTable[];
}

const COLORS = [
  '#3b82f6', // blue
  '#10b981', // emerald
  '#f59e0b', // amber
  '#ef4444', // red
  '#8b5cf6', // purple
  '#ec4899', // pink
  '#06b6d4', // cyan
];

export const ChartModal: React.FC<ChartModalProps> = ({ isOpen, onClose, tables }) => {
  const [chartTitle, setChartTitle] = useState('Sales by Region');
  const [chartType, setChartType] = useState<ChartType>('Column');
  const [sourceTable, setSourceTable] = useState('');
  const [categoryCol, setCategoryCol] = useState('');
  const [valueCols, setValueCols] = useState<string[]>([]);
  const lastRequest = useRef(0);
  const selectedTable = tables.find(t => t.name === sourceTable);
  const availableColumns = selectedTable?.columns ?? [];

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [chartData, setChartData] = useState<ChartData | null>(null);

  useEffect(() => {
    if (!isOpen) {
      lastRequest.current++;
      setLoading(false);
      return;
    }
    if (!selectedTable && tables.length) handleTableChange(tables[0].name);
  }, [isOpen, tables, selectedTable]);

  const handleTableChange = (name: string) => {
    setSourceTable(name);
    const columns = tables.find(t => t.name === name)?.columns ?? [];
    setCategoryCol(columns[0] ?? '');
    setValueCols(columns[1] ? [columns[1]] : []);
    setChartData(null);
  };

  const seriesCols = chartType === 'Pie' ? valueCols.slice(0, 1) : valueCols;
  const canGenerate = selectedTable && availableColumns.includes(categoryCol) &&
    seriesCols.length > 0 && seriesCols.every(column => availableColumns.includes(column));
  const handleGenerate = async () => {
    if (!canGenerate || loading || !selectedTable) return;
    const request = ++lastRequest.current;
    setLoading(true);
    setError(null);
    try {
      const def: ChartDefinition = {
        id: `chart_${Date.now()}`,
        title: chartTitle,
        chart_type: chartType,
        source_table: selectedTable.source_name,
        category_column: categoryCol,
        series: seriesCols.map(column => ({ name: column, value_column: column })),
      };
      await api.saveChart(def);
      const data = await api.getChartData(def.id);
      if (request === lastRequest.current) setChartData(data);
    } catch (err) {
      if (request === lastRequest.current) setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (request === lastRequest.current) setLoading(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4">
      <div role="dialog" aria-modal="true" aria-label="Chart Builder" className="bg-white rounded-lg shadow-xl w-full max-w-4xl h-[580px] flex flex-col overflow-hidden border border-slate-200">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-slate-200 bg-slate-50">
          <div className="flex items-center gap-2">
            <BarChart2 className="w-5 h-5 text-blue-600" />
            <h2 className="text-sm font-bold text-slate-800">Chart Visualization Studio</h2>
          </div>
          <button
            aria-label="Close chart builder"
            onClick={onClose}
            className="text-slate-400 hover:text-slate-600 p-1 rounded hover:bg-slate-200"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {error && (
          <div className="bg-rose-50 text-rose-800 text-xs px-4 py-2 border-b border-rose-200 flex justify-between items-center">
            <span>{error}</span>
            <button onClick={() => setError(null)} className="font-bold underline text-[10px]">
              Dismiss
            </button>
          </div>
        )}

        {/* Content Body */}
        <div className="flex-1 flex overflow-hidden">
          {/* Left Config Panel */}
          <fieldset disabled={loading} className="w-80 shrink-0 border-r border-slate-200 p-4 bg-slate-50/50 flex flex-col gap-3 text-xs overflow-auto">
            <div>
              <label className="block text-[11px] font-bold text-slate-700 mb-1" htmlFor="chart-title">Chart Title</label>
              <input id="chart-title"
                type="text"
                value={chartTitle}
                onChange={(e) => setChartTitle(e.target.value)}
                className="w-full px-2.5 py-1.5 border border-slate-300 rounded bg-white text-xs"
              />
            </div>

            <div>
              <label className="block text-[11px] font-bold text-slate-700 mb-1" htmlFor="chart-type">Chart Type</label>
              <select id="chart-type"
                value={chartType}
                onChange={(e) => setChartType(e.target.value as ChartType)}
                className="w-full px-2.5 py-1.5 border border-slate-300 rounded bg-white text-xs font-semibold"
              >
                <option value="Column">Column Chart</option>
                <option value="Bar">Bar Chart</option>
                <option value="Line">Line Chart</option>
                <option value="Pie">Pie Chart</option>
                <option value="Scatter">Scatter Plot</option>
              </select>
            </div>

            <div>
              <label className="block text-[11px] font-bold text-slate-700 mb-1" htmlFor="chart-source">Source Table</label>
              <select id="chart-source"
                value={sourceTable}
                onChange={(e) => handleTableChange(e.target.value)}
                className="w-full px-2.5 py-1.5 border border-slate-300 rounded bg-white text-xs font-medium"
              >
                {tables.map((t) => (
                  <option key={t.name} value={t.name}>
                    {t.name}
                  </option>
                ))}
              </select>
            </div>

            <FieldWells columns={availableColumns} wells={[
              {label: 'Categories', fields: categoryCol ? [categoryCol] : [], onChange: fields => setCategoryCol(fields[0] ?? '')},
              {label: 'Values', fields: seriesCols, onChange: setValueCols, multiple: chartType !== 'Pie'},
            ]} />

            <div className="pt-2 mt-auto">
              <button
                onClick={handleGenerate}
                disabled={loading || !canGenerate}
                className="w-full flex items-center justify-center gap-1.5 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded font-bold shadow-sm transition-colors disabled:opacity-50"
              >
                <Play className="w-3.5 h-3.5 fill-white" />
                <span>{loading ? 'Rendering...' : 'Render Chart'}</span>
              </button>
            </div>
          </fieldset>

          {/* Right SVG Chart Viewer */}
          <div className="flex-1 p-6 bg-white overflow-auto flex flex-col items-center justify-center">
            {chartData && chartData.series.some(item => item.values.length > 0) ? (
              <div className="w-full h-full flex flex-col">
                <div className="text-center font-bold text-slate-800 text-sm mb-4">
                  {chartData.title}
                </div>
                <div className="flex-1 relative flex items-center justify-center">
                  <SvgChartViewer chartData={chartData} />
                </div>
              </div>
            ) : (
              <div className="flex flex-col items-center justify-center text-slate-400 text-xs">
                <BarChart2 className="w-10 h-10 text-slate-300 mb-2" />
                <span>{chartData ? 'No chart data matched the source table.' : 'Select a table and fields, then click Render Chart.'}</span>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

// Interactive SVG Chart Renderer
const SvgChartViewer: React.FC<{ chartData: ChartData }> = ({ chartData }) => {
  const { chart_type, categories, series } = chartData;
  const values = series[0]?.values || [];
  const { min, max, fraction } = signedScale(series.map(item => item.values));

  const width = 500;
  const height = 300;
  const padding = 40;

  if (chart_type === 'Pie') {
    const total = values.reduce((sum, v) => sum + Math.max(0, v), 0) || 1;
    let accumulatedAngle = 0;
    const cx = width / 2;
    const cy = height / 2;
    const r = Math.min(width, height) / 2 - 30;

    return (
      <svg width={width} height={height} className="overflow-visible">
        {values.map((v, i) => {
          const sliceAngle = (Math.max(0, v) / total) * 2 * Math.PI;
          const startAngle = accumulatedAngle;
          accumulatedAngle += sliceAngle;
          const endAngle = accumulatedAngle;

          const x1 = cx + r * Math.cos(startAngle);
          const y1 = cy + r * Math.sin(startAngle);
          const x2 = cx + r * Math.cos(endAngle);
          const y2 = cy + r * Math.sin(endAngle);
          const largeArc = sliceAngle > Math.PI ? 1 : 0;

          const pathData = `M ${cx} ${cy} L ${x1} ${y1} A ${r} ${r} 0 ${largeArc} 1 ${x2} ${y2} Z`;
          const color = COLORS[i % COLORS.length];

          return (
            <g key={i}>
              <path d={pathData} fill={color} stroke="#ffffff" strokeWidth="2" className="hover:opacity-80 transition-opacity" />
              <title>{`${categories[i] || `Item ${i}`}: ${v}`}</title>
            </g>
          );
        })}
      </svg>
    );
  }

  const horizontal = chart_type === 'Bar';
  const grouped = horizontal || chart_type === 'Column';
  const allLength = series.reduce((size, item) => Math.max(size, item.values.length), categories.length);
  const left = horizontal ? 106 : padding;
  const right = width - padding;
  const bottom = height - padding;
  const groupStep = (horizontal ? bottom - padding : right - left) / Math.max(allLength, 1);
  const groupInset = groupStep * 0.15;
  const thickness = groupStep * 0.7 / Math.max(series.length, 1);
  const project = (v: number) => horizontal
    ? left + fraction(v) * (right - left)
    : bottom - fraction(v) * (bottom - padding);
  const groupCenter = (i: number) => (horizontal ? padding : left) + (i + 0.5) * groupStep;
  const ticks = [...new Set([min, min + (max - min) / 4, (min + max) / 2, max - (max - min) / 4, max, 0])];
  const tickLabel = new Intl.NumberFormat(undefined, {maximumSignificantDigits: 4});
  return <div className="min-w-0">
    <svg viewBox={`0 0 ${width} ${height}`} width={width} height={height} role="img"
      aria-label={`${chart_type} chart: ${chartData.title}`} className="max-w-full">
      {ticks.map(value => {
        const at = project(value);
        return <g key={value}>
          {horizontal
            ? <line x1={at} y1={padding} x2={at} y2={bottom} stroke={value === 0 ? '#64748b' : '#e2e8f0'} />
            : <line x1={left} y1={at} x2={right} y2={at} stroke={value === 0 ? '#64748b' : '#e2e8f0'} />}
          {value !== 0 || min === 0 || max === 0
            ? <text x={horizontal ? at : left - 6} y={horizontal ? bottom + 15 : at + 3}
                textAnchor={horizontal ? 'middle' : 'end'} className="text-[9px] fill-slate-500">
                {tickLabel.format(value)}
              </text>
            : null}
        </g>;
      })}
      {categories.map((category, i) => (i % Math.max(1, Math.ceil(categories.length / 12)) === 0) &&
        <text key={i} x={horizontal ? left - 7 : groupCenter(i)} y={horizontal ? groupCenter(i) + 3 : bottom + 15}
          textAnchor={horizontal ? 'end' : 'middle'} className="text-[9px] fill-slate-700">
          <title>{category}</title>{category.slice(0, horizontal ? 16 : 8)}
        </text>)}
      {series.map(({ name, values: points }, s) => {
        const color = COLORS[s % COLORS.length];
        return <g key={s}>
          {chart_type === 'Line' && <polyline fill="none" stroke={color} strokeWidth="2.5"
            points={points.map((v, i) => `${groupCenter(i)},${project(v)}`).join(' ')} />}
          {points.map((value, i) => {
            const tooltip = `${categories[i] ?? i + 1} · ${name}: ${value}`;
            const anchor = (horizontal ? padding : left) + i * groupStep + groupInset + s * thickness;
            if (grouped) {
              const { start, length } = barSpan(value, project);
              if (value === 0) {
                return horizontal
                  ? <line key={i} data-chart-mark="zero" x1={start} x2={start} y1={anchor} y2={anchor + thickness} stroke={color} strokeWidth="2"><title>{tooltip}</title></line>
                  : <line key={i} data-chart-mark="zero" x1={anchor} x2={anchor + thickness} y1={start} y2={start} stroke={color} strokeWidth="2"><title>{tooltip}</title></line>;
              }
              return <rect key={i} data-chart-mark="bar" x={horizontal ? start : anchor} y={horizontal ? anchor : start}
                width={horizontal ? length : thickness} height={horizontal ? thickness : length}
                fill={color} className="hover:opacity-80"><title>{tooltip}</title></rect>;
            }
            return <circle key={i} cx={groupCenter(i)} cy={project(value)} r={chart_type === 'Line' ? 3 : 4}
              fill={color}><title>{tooltip}</title></circle>;
          })}
        </g>;
      })}
    </svg>
    {series.length > 1 && <ul aria-label="Chart series" className="flex flex-wrap justify-center gap-3 text-xs">
      {series.map(({ name }, i) => <li key={i}><span aria-hidden="true" style={{ color: COLORS[i % COLORS.length] }}>● </span>{name}</li>)}
    </ul>}
  </div>;
};
