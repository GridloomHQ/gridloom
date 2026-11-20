import React, { useState, useEffect, useRef } from 'react';
import { X, PieChart, RefreshCw, Table as TableIcon } from 'lucide-react';
import { api } from '../api';
import { PivotAggregation, PivotDefinition, PivotResult, WorksheetTable } from '../types';
import { FieldWells } from './FieldWells';
import { displayValue } from '../utils/values';

interface PivotBuilderModalProps {
  isOpen: boolean;
  onClose: () => void;
  tables: WorksheetTable[];
}

export const PivotBuilderModal: React.FC<PivotBuilderModalProps> = ({ isOpen, onClose, tables }) => {
  const [saved, setSaved] = useState<PivotDefinition[]>([]);
  const [selected, setSelected] = useState<PivotDefinition | null>(null);
  const [pivotName, setPivotName] = useState('');
  const [sourceTable, setSourceTable] = useState('');
  const [rowFields, setRowFields] = useState<string[]>([]);
  const [colFields, setColFields] = useState<string[]>([]);
  const [valField, setValField] = useState('');
  const [aggregation, setAggregation] = useState<PivotAggregation>('Sum');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<{ data: PivotResult; origin: string } | null>(null);
  const request = useRef<AbortController | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    const controller = new AbortController();
    request.current = controller;
    setSaved([]);
    setSelected(null);
    setResult(null);
    setError(null);
    setPivotName('');
    setSourceTable('');
    setRowFields([]);
    setColFields([]);
    setValField('');
    setAggregation('Sum');
    setLoading(true);
    api.getPivots(controller.signal)
      .then(pivots => { if (!controller.signal.aborted) setSaved(pivots); })
      .catch((err: Error) => { if (!controller.signal.aborted) setError(err.message); })
      .finally(() => { if (!controller.signal.aborted) setLoading(false); });
    return () => request.current?.abort();
  }, [isOpen]);

  const table = tables.find(t => t.name === sourceTable);
  const columns = table?.columns ?? [];
  const setTable = (name: string) => {
    setSourceTable(name);
    const cols = tables.find(t => t.name === name)?.columns ?? [];
    setRowFields(cols[0] ? [cols[0]] : []);
    setColFields([]);
    setValField(cols[cols.length - 1] ?? '');
  };

  const begin = () => {
    request.current?.abort();
    const controller = new AbortController();
    request.current = controller;
    setLoading(true);
    setError(null);
    return controller;
  };
  const handleError = (err: unknown, controller: AbortController) => {
    if (!controller.signal.aborted) setError(err instanceof Error ? err.message : String(err));
  };

  const loadSaved = async (name: string) => {
    const def = saved.find(p => p.name === name) ?? null;
    setSelected(def);
    setResult(null);
    if (!def) {
      request.current?.abort();
      setLoading(false);
      setError(null);
      setPivotName('');
      setTable('');
      setAggregation('Sum');
      return;
    }
    setPivotName(def.name);
    setSourceTable(tables.find(t => t.source_name === def.source_table)?.name ?? '');
    setRowFields(def.row_fields);
    setColFields(def.col_fields);
    setValField(def.value_field);
    setAggregation(def.aggregation);
    const controller = begin();
    try {
      const data = await api.getPivotData(def.name, controller.signal);
      if (!controller.signal.aborted) setResult({ data, origin: 'Workbook source snapshot' });
    } catch (err) {
      handleError(err, controller);
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  };

  const handleSave = async () => {
    if (!table || !valField || !pivotName.trim()) return;
    const controller = begin();
    try {
      const def: PivotDefinition = {
        name: pivotName.trim(),
        source_table: table.source_name,
        row_fields: rowFields,
        col_fields: colFields,
        value_field: valField,
        aggregation,
      };
      const { result: data } = await api.savePivot(def);
      if (!controller.signal.aborted) {
        setSelected(def);
        setPivotName(def.name);
        setSaved(previous => [...previous.filter(p => p.name !== def.name), def].sort((a, b) => a.name.localeCompare(b.name)));
        setResult({ data, origin: 'Workbook source snapshot' });
      }
    } catch (err) {
      handleError(err, controller);
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  };

  const handleRefresh = async () => {
    if (!selected) return;
    const controller = begin();
    try {
      const data = await api.refreshPivot(selected.name, controller.signal);
      if (!controller.signal.aborted) setResult({ data, origin: 'Source query rerun; workbook snapshot unchanged' });
    } catch (err) {
      handleError(err, controller);
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  };

  if (!isOpen) return null;

  const selectClass = 'w-full px-2.5 py-1.5 border border-slate-300 rounded bg-white text-xs';
  const buttonClass = 'px-3 py-2 border border-slate-300 rounded font-semibold disabled:opacity-50';
  const nameInUse = saved.some(p => p.name === pivotName.trim() && p.name !== selected?.name);
  const draftChanged = selected && (
    pivotName !== selected.name || (table && table.source_name !== selected.source_table) ||
    JSON.stringify(rowFields) !== JSON.stringify(selected.row_fields) ||
    JSON.stringify(colFields) !== JSON.stringify(selected.col_fields) || valField !== selected.value_field || aggregation !== selected.aggregation
  );

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4">
      <div role="dialog" aria-modal="true" aria-label="Pivot Table Builder"
        className="bg-white rounded-lg shadow-xl w-full max-w-4xl h-[560px] flex flex-col overflow-hidden border border-slate-200">
        <header className="flex items-center justify-between px-5 py-3 border-b bg-slate-50">
          <h2 className="flex items-center gap-2 text-sm font-bold text-slate-800">
            <PieChart className="w-5 h-5 text-purple-600" /> Pivot Table Builder
          </h2>
          <button aria-label="Close pivot builder" onClick={onClose} className="p-1 rounded hover:bg-slate-200">
            <X className="w-4 h-4" />
          </button>
        </header>
        {error && <div role="alert" className="bg-rose-50 text-rose-800 text-xs px-4 py-2">
          {error}{result ? ' The previous result is still shown.' : ''}
        </div>}
        <div className="flex-1 flex overflow-hidden">
          <fieldset disabled={loading} className="w-80 border-r p-4 bg-slate-50/50 flex flex-col gap-3 text-xs overflow-auto">
            <label>Saved pivot<select className={selectClass} value={selected?.name ?? ''} onChange={e => void loadSaved(e.target.value)}>
              <option value="">New pivot</option>
              {saved.map(p => <option key={p.name}>{p.name}</option>)}
            </select></label>
            <label>Pivot name<input type="text" className={selectClass} value={pivotName} onChange={e => setPivotName(e.target.value)} /></label>
            <label>Source table<select className={selectClass} value={sourceTable} onChange={e => setTable(e.target.value)}>
              <option value="">{selected ? selected.source_table + ' (saved source)' : 'Choose a table'}</option>
              {tables.map(t => <option key={t.name} value={t.name}>{t.name}</option>)}
            </select></label>
            <FieldWells columns={columns} wells={[
              {label: 'Rows', fields: rowFields, onChange: setRowFields, multiple: true},
              {label: 'Columns', fields: colFields, onChange: setColFields, multiple: true},
              {label: 'Values', fields: valField ? [valField] : [], onChange: fields => setValField(fields[0] ?? '')},
            ]} />
            <label>Aggregation<select className={selectClass} value={aggregation} onChange={e => setAggregation(e.target.value as PivotAggregation)}>
              {(['Sum', 'Count', 'Average', 'Min', 'Max'] as const).map(item => <option key={item}>{item}</option>)}
            </select></label>
            <button className={buttonClass} onClick={() => void handleSave()} disabled={!table || ![...rowFields, ...colFields, valField].every(field => columns.includes(field)) || !pivotName.trim() || nameInUse}>
              {pivotName.trim() === selected?.name ? 'Update layout and preview' : 'Save and preview'}
            </button>
            <button className={buttonClass} onClick={() => void handleRefresh()} disabled={!selected}>
              <RefreshCw className="w-3.5 h-3.5 inline mr-1" /> Refresh from source
            </button>
            {nameInUse && <p>Select the existing pivot to update its layout.</p>}
            {draftChanged && <p>Refresh uses the saved layout. Save edits first to refresh them.</p>}
          </fieldset>
          <section className="flex-1 p-4 overflow-auto">
            {loading && <p role="status" className="text-xs pb-2">Working…</p>}
            {result ? <>
              <p className="text-xs text-slate-600 mb-2">{selected?.name} · {result.origin}</p>
              <table className="w-full text-left text-xs border-collapse">
                <thead className="bg-purple-50 text-purple-900 border-b">
                  <tr>{result.data.headers.map((h, i) => <th key={i} className="px-3 py-2">{h}</th>)}</tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  {result.data.rows.map((row, i) => <tr key={i}>
                    {row.map((value, j) => <td key={j} className="px-3 py-1.5">{displayValue(value)}</td>)}
                  </tr>)}
                </tbody>
              </table>
              {!result.data.rows.length && <p className="text-xs p-3">No rows matched the source query.</p>}
            </> : <div className="flex flex-col items-center justify-center text-slate-500 text-xs h-full">
              <TableIcon className="w-8 h-8 mb-2" /> Choose a saved pivot or configure a new layout.
            </div>}
          </section>
        </div>
      </div>
    </div>
  );
};
