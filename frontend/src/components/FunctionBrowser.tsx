import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { browserFunctions, FUNCTION_CATEGORIES, functionHelp } from '../utils/functionCatalog';
import type { FunctionCategory } from '../utils/functionCatalog';

type CategoryFilter = FunctionCategory | 'All' | 'Other';

interface FunctionBrowserProps {
  names: readonly string[];
  status: 'loading' | 'ready' | 'error';
  replacesDraft: boolean;
  onReload: () => void;
  onClose: () => void;
  onInsert: (name: string) => void;
}

export function FunctionBrowser({ names, status, replacesDraft, onReload, onClose, onInsert }: FunctionBrowserProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState<CategoryFilter>('All');
  const [chosen, setChosen] = useState('');
  const filters = FUNCTION_CATEGORIES.filter(group => names.some(name => functionHelp(name)?.category === group));
  const hasOther = names.some(name => !functionHelp(name));
  const results = status === 'ready' ? browserFunctions(names, query, category) : [];
  const selected = results.includes(chosen) ? chosen : results[0];
  const info = selected ? functionHelp(selected) : undefined;

  useLayoutEffect(() => {
    const el = dialog.current!;
    el.showModal();
    search.current?.focus();
    return () => { if (el.open) el.close(); };
  }, []);

  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  const dismiss = () => { dialog.current?.close(); onClose(); };
  const insert = () => {
    if (!selected) return;
    dialog.current?.close();
    onInsert(selected);
  };
  const navigate = (key: string) => {
    const current = results.indexOf(selected);
    const index = key === 'Home' ? 0 : key === 'End' ? results.length - 1
      : Math.min(results.length - 1, Math.max(0, current + (key === 'ArrowDown' ? 1 : -1)));
    if (results[index]) setChosen(results[index]);
  };
  const handleKeys = (e: KeyboardEvent<HTMLDialogElement>) => {
    e.stopPropagation();
    if (e.nativeEvent.isComposing || e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
    if (e.key === 'Escape') { e.preventDefault(); dismiss(); return; }
    if (e.target !== search.current && e.target !== list.current) return;
    if (!results.length) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp' ||
       (e.target === list.current && (e.key === 'Home' || e.key === 'End'))) {
      e.preventDefault();
      navigate(e.key);
    } else if (e.key === 'Enter' && !replacesDraft) {
      e.preventDefault();
      insert();
    }
  };

  return <dialog ref={dialog} id="function-picker" aria-labelledby="function-browser-title"
    onCancel={e => { e.preventDefault(); dismiss(); }} onKeyDown={handleKeys}
    className="m-auto w-[min(94vw,48rem)] max-h-[90vh] overflow-y-auto rounded-lg border border-slate-200 p-4 shadow-xl backdrop:bg-black/30">
    <header className="mb-3 flex items-start justify-between gap-4">
      <div>
        <h2 id="function-browser-title" className="text-base font-semibold text-slate-900">Insert function</h2>
        <p className="text-slate-600">Browse supported functions and insert one into the formula draft.</p>
      </div>
      <button type="button" onClick={dismiss} aria-label="Close function browser"
        className="rounded border px-2 py-1 hover:bg-slate-100">Close</button>
    </header>
    <div className="mb-3 grid grid-cols-1 gap-2 sm:grid-cols-[2fr_1fr]">
      <label className="grid gap-1 text-slate-700">Search functions
        <input ref={search} aria-controls={status === 'ready' ? 'function-browser-list' : undefined} value={query} placeholder="Name or description"
          onChange={e => { setQuery(e.target.value); setChosen(''); }} className="min-w-0 rounded border p-2" />
      </label>
      <label className="grid gap-1 text-slate-700">Category
        <select value={category} onChange={e => { setCategory(e.target.value as CategoryFilter); setChosen(''); }}
          className="min-w-0 rounded border bg-white p-2">
          <option value="All">All categories</option>
          {filters.map(group => <option key={group}>{group}</option>)}
          {hasOther && <option>Other</option>}
        </select>
      </label>
    </div>
    {status === 'loading' && <p role="status" className="p-2">Loading functions…</p>}
    {status === 'error' && <div className="flex items-center gap-2 p-2">
      <p role="status">Couldn’t load functions. You can still type formulas manually.</p>
      <button type="button" onClick={onReload} className="rounded border px-2 py-1 hover:bg-slate-100">Retry</button>
    </div>}
    {status === 'ready' && <div className="grid grid-cols-1 gap-3 sm:grid-cols-[1fr_2fr]">
      <div ref={list} id="function-browser-list" role="listbox" aria-label="Available functions" tabIndex={results.length ? 0 : undefined}
        aria-activedescendant={selected ? `function-browser-${selected}` : undefined}
        className="max-h-64 min-h-24 overflow-y-auto rounded border sm:max-h-80">
        {results.map(name => <div role="option" id={`function-browser-${name}`} key={name} aria-selected={name === selected}
          onClick={() => { setChosen(name); list.current?.focus(); }}
          className={`cursor-pointer px-3 py-2 font-mono hover:bg-slate-100 ${name === selected ? 'bg-emerald-50 font-bold text-emerald-900' : ''}`}>
          {name}
        </div>)}
        {!results.length && <p role="status" className="p-3 text-slate-600">No matching functions.</p>}
      </div>
      <div aria-live="polite" role="region" aria-label="Function details" tabIndex={selected ? 0 : undefined} className="max-h-80 min-w-0 overflow-y-auto rounded border bg-slate-50 p-3">
        {selected && <>
          <h3 className="mb-1 text-sm font-semibold">{selected}</h3>
          {info ? <>
            <p className="mb-2 text-slate-700">{info.description}</p>
            <h4 className="font-semibold">Syntax</h4>
            <code className="mb-1 block break-words">{info.syntax}</code>
            {info.aliases.length > 0 && <p className="mb-2 text-slate-600">Also available as {info.aliases.join(', ')}.</p>}
            <h4 className="mt-2 font-semibold">Inputs</h4>
            {info.arguments.length ? <dl className="space-y-1">{info.arguments.map(arg =>
              <div key={arg.name}><dt className="inline font-mono font-semibold">{arg.name}: </dt><dd className="inline">{arg.detail}</dd></div>
            )}</dl> : <p>No inputs.</p>}
            <h4 className="mt-2 font-semibold">Returns</h4>
            <p>{info.returns}</p>
            <h4 className="mt-2 font-semibold">Example</h4>
            <code className="block break-words">{info.example}</code>
          </> : <>
            <code className="block">={selected}(…)</code>
            <p>This function is supported by the engine, but detailed help is not available yet.</p>
          </>}
        </>}
      </div>
    </div>}
    <footer className="mt-3 flex items-center justify-between gap-3 border-t pt-3">
      <p className="text-slate-600">{replacesDraft && selected ? 'There is no function slot at the caret. Replace draft to start over, or close the browser to keep editing.'
        : '↑ / ↓ browse. Enter inserts. Escape closes.'}</p>
      <button type="button" onClick={insert} disabled={!selected}
        className="rounded bg-emerald-700 px-3 py-1.5 text-white disabled:bg-slate-300">{replacesDraft ? 'Replace draft' : 'Insert'}</button>
    </footer>
  </dialog>;
}
