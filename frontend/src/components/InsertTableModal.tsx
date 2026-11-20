import React, { useState, useEffect } from 'react';
import { X, Table as TableIcon, ArrowRight } from 'lucide-react';
import { api } from '../api';
import { NamedSource } from '../types';
import { WorksheetPlacement } from './WorksheetPlacement';
import { parsePlacement, type GridPlacement, type PlacementDraft } from '../utils/worksheetPlacement';

interface InsertTableModalProps {
  isOpen: boolean;
  onClose: () => void;
  sheets: readonly string[];
  selection: GridPlacement;
  onTableInserted: () => void;
}

export const InsertTableModal: React.FC<InsertTableModalProps> = ({
  isOpen,
  onClose,
  sheets,
  selection,
  onTableInserted,
}) => {
  const [sources, setSources] = useState<NamedSource[]>([]);
  const [selectedSource, setSelectedSource] = useState('');
  const [tableName, setTableName] = useState('');
  const [placement, setPlacement] = useState<PlacementDraft>({sheetName: '', anchor: ''});
  const destination = parsePlacement(placement, sheets);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    let active = true;
    setPlacement({sheetName: '', anchor: ''});
    setSelectedSource('');
    setTableName('');
    setSources([]);
    setError(null);
    api.getSources().then((srcs) => {
      if (!active) return;
      setSources(srcs);
      if (srcs.length > 0) {
        setSelectedSource(srcs[0].name);
        setTableName(srcs[0].name);
      }
    }).catch((err: Error) => {
      if (active) setError(err.message);
    });
    return () => { active = false; };
  }, [isOpen]);

  const handleSourceChange = (name: string) => {
    setSelectedSource(name);
    setTableName(name);
  };

  const handleInsert = async (e: React.FormEvent) => {
    e.preventDefault();
    if (loading || !selectedSource || !tableName.trim() || !destination) return;
    setLoading(true);
    setError(null);
    try {
      await api.insertTable(destination.sheetName, tableName.trim(), selectedSource, destination.row, destination.col);
      onTableInserted();
      onClose();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4">
      <div role="dialog" aria-modal="true" aria-labelledby="insert-table-heading" className="bg-white rounded-lg shadow-xl w-full max-w-md flex flex-col overflow-hidden border border-slate-200">
        <div className="flex items-center justify-between px-5 py-3 border-b border-slate-200 bg-slate-50">
          <div className="flex items-center gap-2">
            <TableIcon className="w-5 h-5 text-emerald-600" />
            <h2 id="insert-table-heading" className="text-sm font-bold text-slate-800">Insert Worksheet Table</h2>
          </div>
          <button
            disabled={loading}
            aria-label="Close insert table dialog"
            onClick={onClose}
            className="text-slate-400 hover:text-slate-600 p-1 rounded hover:bg-slate-200"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {error && (
          <div role="alert" className="bg-rose-50 text-rose-800 text-xs px-4 py-2 border-b border-rose-200">
            {error}
          </div>
        )}

        <form onSubmit={handleInsert} className="p-4 space-y-3 text-xs">
          <div>
            <label className="block text-[11px] font-bold text-slate-700 mb-1">
              Select Data Source
            </label>
            {sources.length === 0 ? (
              <div className="text-slate-500 italic p-2 border border-slate-200 rounded bg-slate-50">
                No named sources defined yet. Use the SQL Query Editor to create a source first.
              </div>
            ) : (
              <select
                value={selectedSource}
                disabled={loading}
                onChange={(e) => handleSourceChange(e.target.value)}
                className="w-full px-2.5 py-1.5 border border-slate-300 rounded bg-white text-xs font-medium"
              >
                {sources.map((s) => (
                  <option key={s.name} value={s.name}>
                    {s.name} ({s.connection_name})
                  </option>
                ))}
              </select>
            )}
          </div>

          <div>
            <label className="block text-[11px] font-bold text-slate-700 mb-1">
              Table Name (Structured Reference)
            </label>
            <input
              type="text"
              value={tableName}
              disabled={loading}
              onChange={(e) => setTableName(e.target.value)}
              placeholder="e.g. Invoices"
              required
              className="w-full px-2.5 py-1.5 border border-slate-300 rounded bg-white text-xs font-mono"
            />
            <span className="text-[10px] text-slate-400 mt-0.5 block">
              Reference columns in formulas with e.g. <code className="text-slate-700">{tableName || 'Table'}[Column]</code>
            </span>
          </div>

          <WorksheetPlacement sheets={sheets} selection={selection} draft={placement} onChange={setPlacement} disabled={loading} />

          <div className="pt-3 border-t border-slate-200 flex items-center justify-end gap-2">
            <button
              type="button"
              disabled={loading}
              onClick={onClose}
              className="px-3 py-1.5 rounded border border-slate-300 text-xs text-slate-600 hover:bg-slate-100 font-medium"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={loading || !selectedSource || !tableName.trim() || !destination}
              className="flex items-center gap-1.5 px-4 py-1.5 bg-emerald-600 hover:bg-emerald-700 text-white rounded text-xs font-bold shadow-sm transition-colors disabled:opacity-50"
            >
              <span>Insert Table</span>
              <ArrowRight className="w-3.5 h-3.5" />
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
