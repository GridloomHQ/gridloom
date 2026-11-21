import { useEffect, useRef, useState } from 'react';
import type { FormEvent } from 'react';
import { api } from '../api';
import type { LocalColumn, LocalColumnType } from '../types';
import { colIndexToName, coordToA1 } from '../utils/coords';
import type { GridRange } from '../utils/selection';
import { worksheetCommandClass } from './Toolbar';

const kinds: LocalColumnType[] = ['Any', 'Text', 'Integer', 'Decimal', 'Boolean', 'Date'];

export function LocalTableDialog({sheet,range,onClose,onCreated}: {
  sheet: string; range: GridRange; onClose: ()=>void; onCreated: (name: string)=>Promise<void>;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name,setName] = useState('');
  const [columns,setColumns] = useState<LocalColumn[]>([]);
  const [error,setError] = useState('');
  const [busy,setBusy] = useState(false);
  const width = range.end.col-range.start.col+1;
  const height = range.end.row-range.start.row+1;
  const valid = height>=2 && width<=50 && width*height<=10_000;
  useEffect(()=>{
    const element = dialog.current;
    element?.showModal();
    return ()=>{element?.close()};
  },[]);
  useEffect(()=>{
    if(!valid)return;
    let current=true;
    api.readRange(sheet,{start:range.start,end:{row:range.start.row,col:range.end.col}}).then(
      ([headings])=>{if(current)setColumns(Array.from({length:width},(_,i)=>({
        name:headings?.[i]||'Column '+colIndexToName(range.start.col+i),kind:'Any',
      })))},
      err=>{if(current)setError(err instanceof Error?err.message:String(err))},
    );
    return ()=>{current=false};
  },[sheet,range,valid,width]);
  async function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if(!valid||busy||columns.length!==width)return;
    setBusy(true);setError('');
    try {
      await api.createLocalTable(sheet,name.trim(),range,columns);
    } catch(err) {
      setBusy(false);
      setError(err instanceof Error?err.message:String(err));
      return;
    }
    await onCreated(name.trim());
  }
  return <dialog ref={dialog} aria-labelledby="local-table-title"
    onCancel={event=>{event.preventDefault();if(!busy)onClose()}}
    className="w-[min(36rem,calc(100vw-2rem))] max-h-[85vh] rounded-lg border border-slate-300 bg-white p-5 text-slate-900 shadow-xl backdrop:bg-slate-900/50">
    <form onSubmit={create} className="space-y-4">
      <h2 id="local-table-title" className="text-lg font-semibold">Create local table</h2>
      <p className="text-sm">{sheet}!{coordToA1(range.start.row,range.start.col)}:{coordToA1(range.end.row,range.end.col)} stays in this workbook. The first row is the header.</p>
      {!valid&&<p role="alert" className="text-sm text-rose-700">Select a header and at least one data row, with up to 50 columns and 10,000 cells.</p>}
      {error&&<p role="alert" className="text-sm text-rose-700">{error}</p>}
      {valid&&<>
        <label className="flex flex-col gap-1 text-sm">Table name
          <input autoFocus required maxLength={128} pattern="[A-Za-z_][A-Za-z_0-9]*"
            className="rounded border border-slate-300 p-2" placeholder="e.g. Transactions"
            value={name} onChange={e=>setName(e.target.value)} disabled={busy}/>
        </label>
        <p className="text-sm">Choose Any to allow formulas. Typed columns validate literal entries; Text preserves leading zeroes and dates use YYYY-MM-DD.</p>
        {columns.length===0&&!error&&<p role="status">Reading the selected headings…</p>}
        <div className="space-y-2">
          {columns.map((column,i)=>{
            const letter=colIndexToName(range.start.col+i);
            return <div className="grid grid-cols-[2fr_1fr] gap-2" key={i}>
              <label className="flex flex-col gap-1 text-sm">Column {letter} heading
                <input required maxLength={128} className="min-w-0 rounded border border-slate-300 p-2"
                  value={column.name} disabled={busy}
                  onChange={e=>setColumns(prev=>prev.map((c,j)=>i===j?{...c,name:e.target.value}:c))}/>
              </label>
              <label className="flex flex-col gap-1 text-sm">Column {letter} type
                <select className="min-w-0 rounded border border-slate-300 p-2"
                  value={column.kind} disabled={busy}
                  onChange={e=>setColumns(prev=>prev.map((c,j)=>i===j?{...c,kind:e.target.value as LocalColumnType}:c))}>
                  {kinds.map(kind=><option key={kind}>{kind}</option>)}
                </select>
              </label>
            </div>;
          })}
        </div>
      </>}
      <div className="flex justify-end gap-2">
        <button type="button" className={worksheetCommandClass} disabled={busy} onClick={onClose}>Cancel</button>
        <button type="submit" className={worksheetCommandClass} disabled={!valid||busy||columns.length!==width}>{busy?'Creating…':'Create table'}</button>
      </div>
    </form>
  </dialog>;
}
