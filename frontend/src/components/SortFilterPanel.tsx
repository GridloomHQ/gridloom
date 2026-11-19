import { useEffect, useRef, useState } from 'react';
import { api } from '../api';
import type { RangeView, RangeViewFilter, RangeViewOptions } from '../api';
import { colIndexToName, coordToA1 } from '../utils/coords';
import { boundedRangeDimensions } from '../utils/selection';
import type { GridRange } from '../utils/selection';

const INITIAL:RangeViewOptions={first_row_header:false,sort:null,filter:null};
const PAGE_SIZE=50;

export function SortFilterPanel({sheetName,range,onClose,onGoTo}:{
  sheetName:string;range:GridRange;onClose:()=>void;onGoTo:(row:number,col:number)=>void;
}) {
  const dialog=useRef<HTMLDialogElement>(null);
  const original=document.activeElement;
  useEffect(()=>{
    const el=dialog.current!;
    el.showModal();
    return ()=>{el.close();if(original instanceof HTMLElement && original.isConnected)original.focus();};
  },[]);
  const [header,setHeader]=useState(false);
  const [sortCol,setSortCol]=useState('');
  const [direction,setDirection]=useState('asc');
  const [filterCol,setFilterCol]=useState('');
  const [mode,setMode]=useState<RangeViewFilter>('contains');
  const [needle,setNeedle]=useState('');
  const [options,setOptions]=useState(INITIAL);
  const [result,setResult]=useState<RangeView|null>(null);
  const [error,setError]=useState('');
  const [loading,setLoading]=useState(true);
  const [page,setPage]=useState(0);

  let columns:number[]=[];
  let rangeError='';
  try {
    const {rows,columns:count}=boundedRangeDimensions(range,5000);
    if(rows>1000 || count>50) throw new Error('Select at most 1,000 rows and 50 columns.');
    columns=Array.from({length:count},(_,i)=>range.start.col+i);
  } catch(e) { rangeError=e instanceof Error?e.message:String(e); }
  useEffect(()=>{
    if(rangeError)return;
    const controller=new AbortController();
    setResult(null);setLoading(true);setError('');setPage(0);
    void api.rangeView(sheetName,range,options,controller.signal).then(view=>{
      if(!controller.signal.aborted){setResult(view);setLoading(false);}
    },e=>{
      if(!controller.signal.aborted){setError(e.message??String(e));setLoading(false);}
    });
    return ()=>controller.abort();
  },[sheetName,range,options,rangeError]);

  const numeric=mode==='at_least'||mode==='at_most';
  const usesText=mode==='contains'||mode==='equals'||numeric;
  const apply=()=>{
    setResult(null);
    setOptions({
      first_row_header:header,sort:sortCol===''?null:{col:Number(sortCol),descending:direction==='desc'},
      filter:filterCol===''?null:{col:Number(filterCol),mode,text:usesText?needle:''},
    });
  };
  const clear=()=>{
    setHeader(false);setSortCol('');setDirection('asc');setFilterCol('');setMode('contains');setNeedle('');
    setResult(null);setOptions({...INITIAL});
  };
  const goTo=async(row:number,col:number)=>{
    if(!result || loading)return;
    const generation=result.generation;
    setLoading(true);setError('');
    try {
      const current=await api.getViewport(sheetName,row,col,row,col,0);
      const isOpen=dialog.current?.open===true;
      if(!isOpen)return;
      if(current.generation!==generation) {
        setResult(null);
        setError('Worksheet has changed. Apply the view again before opening a row.');
      } else onGoTo(row,col);
    } catch(e) {
      if(dialog.current?.open)setError(e instanceof Error?e.message:String(e));
    } finally {
      if(dialog.current?.open)setLoading(false);
    }
  };
  const columnChoices=columns.map((col,i)=><option key={col} value={col}>{result?.headers[i]??colIndexToName(col)}</option>);
  const name=`${sheetName}!${coordToA1(range.start.row,range.start.col)}:${coordToA1(range.end.row,range.end.col)}`;
  return <dialog className="m-auto w-[90vw] max-w-5xl rounded-lg border bg-white p-4 shadow-xl backdrop:bg-black/30"
    ref={dialog} aria-labelledby="sort-filter-title" onCancel={e=>{e.preventDefault();onClose();}} onKeyDown={e=>{e.stopPropagation();if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='s')e.preventDefault();}}>
    <section className="flex max-h-[82vh] min-h-[16rem] flex-col gap-3 text-sm">
      <header className="flex items-start justify-between gap-3">
        <div><h2 className="text-lg font-semibold" id="sort-filter-title">Sort &amp; filter</h2>
          <p className="font-mono text-xs text-slate-600">{name}</p></div>
        <button className="rounded border px-3 py-1" onClick={onClose} autoFocus>Close</button>
      </header>
      <p>Read-only view of the selection. Rows stay in their original positions in the worksheet, and formulas and database links don’t change.</p>
      {!rangeError&&<form className="flex flex-wrap items-end gap-2" onSubmit={e=>{e.preventDefault();apply();}}>
        <label className="mb-1 flex items-center gap-1"><input type="checkbox" checked={header} onChange={e=>setHeader(e.target.checked)}/> First row is headers</label>
        <label className="flex flex-col">Sort by<select value={sortCol} onChange={e=>setSortCol(e.target.value)} className="rounded border p-1"><option value="">Original order</option>{columnChoices}</select></label>
        <label className="flex flex-col">Direction<select disabled={sortCol===''} value={direction} onChange={e=>setDirection(e.target.value)} className="rounded border p-1"><option value="asc">Ascending</option><option value="desc">Descending</option></select></label>
        <label className="flex flex-col">Filter by<select value={filterCol} onChange={e=>setFilterCol(e.target.value)} className="rounded border p-1"><option value="">No filter</option>{columnChoices}</select></label>
        <label className="flex flex-col">Condition<select value={mode} disabled={filterCol===''} onChange={e=>setMode(e.target.value as RangeViewFilter)} className="rounded border p-1"><option value="contains">Contains display text</option><option value="equals">Equals display text</option><option value="empty">Is empty</option><option value="non_empty">Is not empty</option><option value="at_least">Exact number ≥</option><option value="at_most">Exact number ≤</option></select></label>
        {usesText&&filterCol!==''&&<label className="flex flex-col">{numeric?"Decimal threshold":"Filter text"}<input value={needle} required onChange={e=>setNeedle(e.target.value)} maxLength={250} className="rounded border p-1"/></label>}
        <button className="rounded border px-3 py-1" type="submit" disabled={loading}>Apply view</button>
        <button className="rounded border px-3 py-1" type="button" disabled={loading} onClick={clear}>Reset view</button>
      </form>}
      <p className="text-xs text-slate-600">Sorts by value type (exact numbers, approximate numbers, dates, timestamps, text, then booleans). Ties keep worksheet order; errors and empty cells stay at the bottom. Text matching is case-insensitive; numeric thresholds exclude approximate values.</p>
      {(rangeError||error)&&<p role="alert" className="text-red-700">{rangeError||error}</p>}
      {loading&&!rangeError&&<p role="status">Loading view…</p>}
      {result&&<>
        <p role="status">{result.rows.length.toLocaleString()} of {result.total_rows.toLocaleString()} data rows</p>
        {result.rows.length===0?<p>No rows match this filter.</p>:<>
          <div className="min-h-0 overflow-auto rounded border">
            <table className="w-full text-left">
              <thead className="sticky top-0 bg-slate-100"><tr><th scope="col" className="whitespace-nowrap p-1">Original row</th>
                {result.headers.map((label,i)=><th scope="col" className="max-w-[18rem] truncate p-1" key={i} title={label}>{label}</th>)}
              </tr></thead>
              <tbody>{result.rows.slice(page*PAGE_SIZE,(page+1)*PAGE_SIZE).map(({row,values})=><tr key={row} className="border-t odd:bg-slate-50">
                <th scope="row" className="p-1"><button disabled={loading} onClick={()=>void goTo(row,range.start.col)} className="font-mono text-blue-700 underline"
                  aria-label={`Go to original row ${row+1}`}>{row+1}</button></th>
                {values.map((v,i)=><td className="max-w-[18rem] truncate p-1" key={i} title={v}>{v}</td>)}
              </tr>)}</tbody>
            </table>
          </div>
          {result.rows.length>PAGE_SIZE&&<nav aria-label="Sort and filter results" className="flex items-center gap-2">
            <button className="rounded border px-2 py-1" disabled={page===0} onClick={()=>setPage(page-1)}>Previous page</button>
            <span>Page {page+1} of {Math.ceil(result.rows.length/PAGE_SIZE)}</span>
            <button className="rounded border px-2 py-1" disabled={(page+1)*PAGE_SIZE>=result.rows.length} onClick={()=>setPage(page+1)}>Next page</button>
          </nav>}
        </>}
      </>}
    </section>
  </dialog>;
}
