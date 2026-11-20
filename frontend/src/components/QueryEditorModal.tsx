import { displayValue } from '../utils/values';
import React, { useState, useEffect, useRef } from 'react';
import { X, Play, ChevronLeft, ChevronRight, CheckCircle, AlertCircle, Sparkles, Table as TableIcon, ArrowRight } from 'lucide-react';
import { api } from '../api';
import { QueryPreviewPage, LineageReport } from '../types';

const PREVIEW_PAGE_SIZE = 100;

interface QueryEditorModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialConnection?: string;
  initialSql?: string;
  onInsertTableFromSource: (sourceName: string) => void;
}

export const QueryEditorModal: React.FC<QueryEditorModalProps> = ({
  isOpen,
  onClose,
  initialConnection,
  initialSql,
  onInsertTableFromSource,
}) => {
  const [connections, setConnections] = useState<{ name: string; kind: string }[]>([]);
  const [connecting, setConnecting] = useState(false);
  const [selectedConn, setSelectedConn] = useState(initialConnection || '');
  const [sql, setSql] = useState(initialSql || 'SELECT * FROM invoices LIMIT 50');
  const [sourceName, setSourceName] = useState('Invoices');

  const [previewLoading, setPreviewLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<QueryPreviewPage | null>(null);
  const [lineage, setLineage] = useState<LineageReport | null>(null);
  const request = useRef<AbortController | null>(null);
  const resultsScroll = useRef<HTMLDivElement>(null);
  const busy = previewLoading || saving;
  const canQuery = !connecting && connections.some(conn => conn.name === selectedConn) && !!sql.trim();

  const clearPreview = () => {
    request.current?.abort();
    request.current = null;
    setPreviewLoading(false);
    setError(null);
    setResult(null);
    setLineage(null);
  };
  const changeSql = (next: string) => {
    if (next === sql) return;
    clearPreview();
    setSql(next);
  };

  useEffect(() => {
    clearPreview();
    if (!isOpen) return;
    let active = true;
    setConnecting(true);
    setConnections([]);
    if (initialConnection) setSelectedConn(initialConnection);
    if (initialSql) setSql(initialSql);
    api.getConnections().then((conns) => {
      if (!active) return;
      setConnections(conns);
      setSelectedConn(current => conns.some(conn => conn.name === current) ? current : conns[0]?.name ?? '');
    }).catch((err: Error) => {
      if (active) setError(err.message);
    }).finally(() => {
      if (active) setConnecting(false);
    });
    return () => {
      active = false;
      request.current?.abort();
      request.current = null;
    };
  }, [isOpen, initialConnection, initialSql]);

  const loadPage = async (offset: number, refreshLineage: boolean) => {
    if (!canQuery || request.current || saving) return;
    const controller = new AbortController();
    request.current = controller;
    setPreviewLoading(true);
    setError(null);
    if (refreshLineage) {
      setResult(null);
      setLineage(null);
    }
    try {
      const [page, analysis] = await Promise.all([
        api.previewQuery(selectedConn, sql, offset, PREVIEW_PAGE_SIZE, controller.signal),
        refreshLineage ? api.analyzeQuery(selectedConn, sql, controller.signal) : Promise.resolve(null),
      ]);
      if (request.current !== controller) return;
      setResult(page);
      if (analysis) setLineage(analysis);
      if (resultsScroll.current) resultsScroll.current.scrollTop = 0;
    } catch (err) {
      if (request.current !== controller) return;
      controller.abort();
      setError(err instanceof Error ? err.message : 'Could not load query preview.');
    } finally {
      if (request.current === controller) {
        request.current = null;
        setPreviewLoading(false);
      }
    }
  };

  const handleSaveAndInsert = async () => {
    if (!canQuery || !sourceName.trim() || !result || busy) return;
    setSaving(true);
    setError(null);
    try {
      await api.createSource(sourceName, selectedConn, sql);
      onInsertTableFromSource(sourceName);
      onClose();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setSaving(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4">
      <div role="dialog" aria-modal="true" aria-labelledby="query-editor-heading" className="bg-white rounded-lg shadow-xl w-full max-w-5xl h-[650px] flex flex-col overflow-hidden border border-slate-200">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-slate-200 bg-slate-50">
          <div className="flex items-center gap-2">
            <Sparkles className="w-5 h-5 text-indigo-600" />
            <h2 id="query-editor-heading" className="text-sm font-bold text-slate-800">SQL Query Editor & Source Builder</h2>
          </div>
          <button
            aria-label="Close query editor"
            onClick={onClose}
            className="text-slate-400 hover:text-slate-600 p-1 rounded hover:bg-slate-200"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {error && (
          <div role="alert" className="bg-rose-50 text-rose-800 text-xs px-4 py-2 border-b border-rose-200 flex justify-between items-center">
            <span className="font-mono">{error}</span>
            <button onClick={() => setError(null)} className="font-bold underline text-[10px]">
              Dismiss
            </button>
          </div>
        )}

        {/* Content Body */}
        <div className="flex-1 flex flex-col p-4 overflow-hidden gap-3">
          {/* Top Controls: Connection & Presets */}
          <div className="flex items-center justify-between gap-3 text-xs">
            <div className="flex items-center gap-2">
              <label htmlFor="query-target-db" className="font-bold text-slate-700">Target Database:</label>
              <select
                id="query-target-db"
                value={connecting || !connections.length ? '' : selectedConn}
                disabled={saving || connecting || !connections.length}
                onChange={e => {
                  clearPreview();
                  setSelectedConn(e.target.value);
                }}
                className="border border-slate-300 rounded px-2.5 py-1 bg-white font-medium text-slate-800"
              >
                {!connections.length && <option value="">{connecting ? 'Loading databases…' : 'No databases connected'}</option>}
                {connections.map((c) => (
                  <option key={c.name} value={c.name}>
                    {c.name} ({c.kind})
                  </option>
                ))}
              </select>
            </div>

            {/* Presets */}
            <div className="flex items-center gap-1.5 text-slate-500">
              <span className="text-[11px] font-semibold">Presets:</span>
              <button
                disabled={saving}
                onClick={() => {
                  changeSql('SELECT id, customer_name, amount, status, issued_on FROM invoices ORDER BY id LIMIT 100');
                  setSourceName('Invoices');
                }}
                className="px-2 py-0.5 bg-slate-100 hover:bg-slate-200 rounded text-[11px] text-slate-700 font-mono"
              >
                Invoices (Editable)
              </button>
              <button
                disabled={saving}
                onClick={() => {
                  changeSql('SELECT id, name, quota, region FROM sales_reps');
                  setSourceName('SalesReps');
                }}
                className="px-2 py-0.5 bg-slate-100 hover:bg-slate-200 rounded text-[11px] text-slate-700 font-mono"
              >
                Sales Reps (Editable)
              </button>
              <button
                disabled={saving}
                onClick={() => {
                  changeSql('SELECT status, count(*) as count, sum(amount) as total_amount FROM invoices GROUP BY status');
                  setSourceName('InvoiceSummary');
                }}
                className="px-2 py-0.5 bg-slate-100 hover:bg-slate-200 rounded text-[11px] text-slate-700 font-mono"
              >
                Summary (Read-Only)
              </button>
            </div>
          </div>

          {/* SQL Input Area */}
          <div className="h-32 border border-slate-300 rounded overflow-hidden flex flex-col focus-within:border-indigo-500">
            <textarea
              aria-label="SQL query"
              value={sql}
              disabled={saving}
              onChange={e => changeSql(e.target.value)}
              placeholder="Enter read-only SQL query (e.g. SELECT * FROM invoices)"
              className="flex-1 w-full p-2.5 font-mono text-xs text-slate-800 bg-slate-50/50 resize-none outline-none leading-relaxed"
            />
            <div className="bg-slate-100 px-3 py-1.5 flex items-center justify-between border-t border-slate-200 text-xs">
              <span className="text-slate-500 text-[11px]">
                Read-only preview, 100 rows per page. Use WHERE to filter and ORDER BY with a unique tie-breaker for consistent paging.
              </span>
              <button
                onClick={() => void loadPage(0, true)}
                disabled={busy || !canQuery}
                className="flex items-center gap-1.5 px-3 py-1 bg-indigo-600 hover:bg-indigo-700 text-white rounded font-semibold transition-colors shadow-sm disabled:opacity-50"
              >
                <Play className="w-3.5 h-3.5 fill-white" />
                <span>{previewLoading ? 'Loading preview…' : 'Run Query'}</span>
              </button>
            </div>
          </div>

          {/* Lineage & Writeback Status Banner */}
          {lineage && (
            <div
              className={`p-2.5 rounded-md border text-xs flex items-center justify-between ${
                lineage.is_editable
                  ? 'bg-emerald-50 border-emerald-200 text-emerald-900'
                  : 'bg-amber-50 border-amber-200 text-amber-900'
              }`}
            >
              <div className="flex items-center gap-2">
                {lineage.is_editable ? (
                  <CheckCircle className="w-4 h-4 text-emerald-600 flex-shrink-0" />
                ) : (
                  <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0" />
                )}
                <div>
                  <div className="font-bold">
                    {lineage.is_editable
                      ? 'Qualified for Safe Staged Writeback'
                      : 'Read-Only Query (Safe View)'}
                  </div>
                  <div className="text-[11px] text-slate-600 mt-0.5">
                    {lineage.is_editable ? (
                      <span>
                        Base table: <span className="font-mono font-semibold">{lineage.base_table}</span> •
                        Identity PK: <span className="font-mono font-semibold">[{lineage.primary_keys.join(', ')}]</span> •
                        Editable columns: {lineage.editable_columns.join(', ')}
                      </span>
                    ) : (
                      <span>
                        {lineage.read_only_reasons.join('; ') || 'Query structure not eligible for in-place row edits.'}
                      </span>
                    )}
                  </div>
                </div>
              </div>

              {lineage.is_editable && (
                <span className="px-2 py-0.5 bg-emerald-600 text-white rounded text-[10px] font-bold uppercase tracking-wider">
                  Editable
                </span>
              )}
            </div>
          )}

          {/* Result Table Preview */}
          <div className="flex-1 min-h-0 border border-slate-200 rounded flex flex-col bg-white" aria-busy={previewLoading}>
            {result && (
              <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 border-b text-xs bg-slate-50">
                <span role="status" className="font-medium text-slate-700">
                  {result.rows.length
                    ? `Rows ${(result.offset + 1).toLocaleString()}–${(result.offset + result.rows.length).toLocaleString()}`
                    : result.offset ? 'No rows at this offset' : 'No matching rows'}
                </span>
                <div role="group" aria-label="Query result pages" className="flex gap-2">
                  <button
                    type="button"
                    disabled={busy || result.offset === 0}
                    onClick={() => void loadPage(Math.max(0, result.offset - PREVIEW_PAGE_SIZE), false)}
                    className="flex items-center gap-1 px-2 py-1 border border-slate-300 rounded bg-white hover:bg-slate-100 disabled:opacity-40"
                  ><ChevronLeft size={14} aria-hidden="true" /> Previous</button>
                  <button
                    type="button"
                    disabled={busy || !result.has_more}
                    onClick={() => void loadPage(result.offset + PREVIEW_PAGE_SIZE, false)}
                    className="flex items-center gap-1 px-2 py-1 border border-slate-300 rounded bg-white hover:bg-slate-100 disabled:opacity-40"
                  >Next <ChevronRight size={14} aria-hidden="true" /></button>
                </div>
              </div>
            )}
            <div ref={resultsScroll} className="flex-1 overflow-auto">
              {result ? (
                <table className="w-full text-left text-xs border-collapse">
                  <thead className="bg-slate-50 border-b border-slate-200 text-slate-600 uppercase text-[10px] sticky top-0">
                    <tr>
                      {result.columns.map((col, index) => (
                        <th scope="col" key={index} className="px-3 py-2 font-semibold">
                          <div className="flex items-center gap-1">
                            <span>{col.name}</span>
                            {col.is_primary_key && (
                              <span className="text-[9px] bg-amber-100 text-amber-800 px-1 rounded font-bold">
                                PK
                              </span>
                            )}
                          </div>
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-100">
                    {result.rows.length === 0 ? (
                      <tr>
                        <td colSpan={result.columns.length} className="px-4 py-8 text-center text-slate-400 italic">
                          Empty result set (0 rows)
                        </td>
                      </tr>
                    ) : (
                      result.rows.map((row, rIdx) => (
                        <tr key={rIdx} className="hover:bg-slate-50">
                          {row.map((val, cIdx) => (
                            <td key={cIdx} className="px-3 py-1.5 font-mono text-slate-700 truncate max-w-[200px]">
                              {val === null ? <span className="text-slate-400 italic">NULL</span> : displayValue(val)}
                            </td>
                          ))}
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              ) : (
                <div className="h-full flex flex-col items-center justify-center text-slate-400 text-xs">
                  <TableIcon className="w-8 h-8 text-slate-300 mb-2" />
                  <span>{previewLoading ? 'Loading query preview…' : 'Run a query to preview results'}</span>
                </div>
              )}
            </div>
          </div>

          {/* Footer: Register as Named Data Source */}
          <div className="pt-2 border-t border-slate-200 flex items-center justify-between">
            <div className="flex items-center gap-2">
              <label htmlFor="query-source-name" className="text-xs font-bold text-slate-700">Source Name:</label>
              <input
                id="query-source-name"
                type="text"
                value={sourceName}
                onChange={(e) => setSourceName(e.target.value)}
                placeholder="e.g. Invoices"
                className="px-2.5 py-1 border border-slate-300 rounded text-xs w-48 font-mono bg-white"
              />
            </div>

            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={onClose}
                className="px-3 py-1.5 rounded border border-slate-300 text-xs text-slate-600 hover:bg-slate-100"
              >
                Close
              </button>
              <button
                onClick={handleSaveAndInsert}
                disabled={busy || !result || !sourceName.trim()}
                className="flex items-center gap-1.5 px-4 py-1.5 bg-emerald-600 hover:bg-emerald-700 text-white rounded text-xs font-semibold shadow-sm transition-colors disabled:opacity-50"
              >
                <span>Save Source & Insert Table</span>
                <ArrowRight className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
