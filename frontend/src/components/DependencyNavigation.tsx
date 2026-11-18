import { useEffect, useState } from 'react';
import { api } from '../api';
import type { DependencyNavigation as Navigation, DependencyTarget } from '../types';

interface Props {
  sheetName: string; row: number; col: number; revision: string;
  onNavigate: (row: number, col: number, sheetName?: string) => void;
  onHighlight: (target: DependencyTarget | null, direction?: 'upstream' | 'downstream') => void;
  onDrilldown?: (name: string) => void;
}
export function DependencyNavigation({ sheetName, row, col, revision, onNavigate, onHighlight, onDrilldown }: Props) {
  const [data, setData] = useState<Navigation | null>(null);
  const [error, setError] = useState('');
  const [direction, setDirection] = useState<'upstream' | 'downstream'>('upstream');
  const [limit, setLimit] = useState(20);
  useEffect(() => {
    let cancelled = false;
    setData(null); setError(''); setLimit(20);
    api.navigateDependencies(sheetName, row, col).then(result => {
      if (!cancelled) setData(result);
    }).catch(reason => { if (!cancelled) setError(String(reason)); });
    return () => { cancelled = true; };
  }, [sheetName, row, col, revision]);
  const current = data?.sheet_name === sheetName && data.coord.row === row && data.coord.col === col ? data : null;
  const neighbors = current?.[direction] ?? [];
  return <section className="space-y-2 border-t pt-3" aria-label="Dependency navigation">
    <h3 className="font-bold">Follow the calculation</h3>
    <div className="flex gap-2" aria-label="Dependency direction">
      {(['upstream', 'downstream'] as const).map(d => <button key={d} type="button" aria-pressed={direction === d}
        className={`border rounded px-2 py-1 ${direction === d ? 'bg-indigo-100 border-indigo-500' : 'bg-white'}`}
        onClick={() => { setDirection(d); setLimit(20); onHighlight(null); }}>{d === 'upstream' ? 'Inputs · what feeds this?' : 'Outputs · what uses this?'}</button>)}
    </div>
    {error && <p role="alert" className="text-rose-700">{error}</p>}
    {!error && !current && <p role="status">Loading direct dependencies…</p>}
    {current && <>
      <p className="text-slate-600">{neighbors.length} direct {direction === 'upstream' ? 'input groups' : 'output groups'}. Follow a cell for the next step. Highlight a range to inspect its members in the grid.</p>
      {neighbors.length === 0 && <p>No {direction === 'upstream' ? 'worksheet inputs' : 'formula consumers'} found within the scope below.</p>}
      <ul className="space-y-2">{neighbors.slice(0, limit).map(target => <li key={target.id} className="bg-white border rounded p-2 space-y-1">
        <div className="font-semibold break-words">{target.label}</div>
        <div className="uppercase text-[10px] text-slate-500">{target.kind}</div>
        {target.formula && <code className="block break-all">{target.formula}</code>}
        <div className="flex gap-3 flex-wrap">
          {(target.range || target.coord) && target.sheet_name && <button type="button" className="text-indigo-700 underline" onClick={() => onHighlight(target, direction)}>{target.sheet_name === sheetName ? 'Highlight on grid' : `Mark highlight on ${target.sheet_name}`}</button>}
          {target.coord && target.sheet_name && <button type="button" className="text-indigo-700 underline" onClick={() => { onHighlight(target, direction); onNavigate(target.coord!.row, target.coord!.col, target.sheet_name!); }}>{target.kind === 'range' || target.kind === 'table' ? 'Go to range and inspect first cell' : 'Go to cell and trace'}</button>}
          {target.source_name && onDrilldown && <button type="button" className="text-indigo-700 underline" onClick={() => onDrilldown(target.source_name!)}>Inspect cached source</button>}
        </div>
      </li>)}</ul>
      {neighbors.length > limit && <button type="button" className="underline text-indigo-700" onClick={() => setLimit(n => n + 20)}>Show next 20 ({Math.min(limit, neighbors.length)} of {neighbors.length})</button>}
      <button type="button" className="text-slate-600 underline" onClick={() => onHighlight(null)}>Clear grid highlight</button>
      <details className="text-slate-600"><summary className="cursor-pointer">Dependency scope {current.truncated ? '(incomplete)' : '(not contribution attribution)'}</summary><ul className="list-disc pl-4 space-y-1">{current.warnings.map(w => <li key={w}>{w}</li>)}</ul></details>
      {current.truncated && <p role="alert" className="text-amber-900">Navigation is incomplete. See dependency scope for limits.</p>}
    </>}
  </section>;
}
