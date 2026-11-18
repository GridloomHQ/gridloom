import React, { useEffect, useRef, useState } from 'react';
import { api } from '../api';
export interface FindMatch {
  sheet_name: string; coord: {row:number;col:number}; address: string;
  display_text: string; raw_input: string | null; formula: string | null;
  is_staged: boolean; is_spill: boolean; table_name: string | null;
}
export interface FindResponse {
  matches: FindMatch[]; total_matches: number; next_offset: number | null;
  scanned_cells: number; truncated: boolean; warnings: string[];
}
export interface FindRequest {
  query:string; sheet_name?:string; look_in:'values'|'formulas';
  case_sensitive:boolean; entire_cell:boolean; offset:number; limit:number;
}
export function FindPanel({sheetName, revision, busy, onNavigate, onClose}: {
  sheetName:string; revision:string; busy:boolean;
  onNavigate:(match:FindMatch)=>void; onClose:()=>void;
}) {
  const [query,setQuery]=useState('');
  const [scope,setScope]=useState('sheet');
  const [lookIn,setLookIn]=useState<'values'|'formulas'>('values');
  const [matchCase,setMatchCase]=useState(false),[entire,setEntire]=useState(false);
  const [result,setResult]=useState<FindResponse|null>(null),[pending,setPending]=useState(false),[error,setError]=useState('');
  const sequence=useRef(0); const mounted=useRef(true); const field=useRef<HTMLInputElement>(null);
  useEffect(()=>{mounted.current=true;field.current?.focus();return()=>{mounted.current=false;sequence.current++;};},[]);
  useEffect(()=>{sequence.current++;setResult(null);setPending(false);setError('');},[query,scope,lookIn,matchCase,entire,revision,scope==='sheet'?sheetName:'']);
  const search=async(offset=0)=>{
    const request=++sequence.current;setPending(true);setError('');
    try {
      const data=await api.findCells({query,sheet_name:scope==='sheet'?sheetName:undefined,look_in:lookIn,case_sensitive:matchCase,entire_cell:entire,offset,limit:50});
      if(mounted.current&&request===sequence.current)setResult(data);
    } catch(e) {if(mounted.current&&request===sequence.current)setError(e instanceof Error?e.message:String(e));}
    finally {if(mounted.current&&request===sequence.current)setPending(false);}
  };
  return <aside aria-label="Find in workbook" className="w-80 shrink-0 border-l bg-white p-3 overflow-auto text-xs" onKeyDown={e=>{if(e.key==='Escape'){e.stopPropagation();onClose();}}}>
    <div className="flex justify-between items-center mb-3"><h2 className="font-semibold">Find</h2><button onClick={onClose} aria-label="Close find">Close</button></div>
    <form onSubmit={e=>{e.preventDefault();if(query&&!pending&&!busy)void search();}} className="space-y-2">
      <label className="block">Find text or number<input ref={field} aria-label="Find text or number" value={query} onChange={e=>setQuery(e.target.value)} className="block border rounded p-1 w-full"/></label>
      <label className="block">Within<select aria-label="Find scope" value={scope} onChange={e=>setScope(e.target.value)} className="ml-2 border"><option value="sheet">Current sheet</option><option value="workbook">Workbook</option></select></label>
      <label className="block">Look in<select aria-label="Find look in" value={lookIn} onChange={e=>setLookIn(e.target.value as 'values'|'formulas')} className="ml-2 border"><option value="values">Displayed values</option><option value="formulas">Formulas / inputs</option></select></label>
      <label className="block"><input type="checkbox" checked={matchCase} onChange={e=>setMatchCase(e.target.checked)}/> Match case</label>
      <label className="block"><input type="checkbox" checked={entire} onChange={e=>setEntire(e.target.checked)}/> Match entire cell</label>
      <button disabled={!query||pending||busy} className="px-3 py-1 bg-blue-600 text-white rounded disabled:opacity-40">{pending?'Searching…':'Find all'}</button>
    </form>
    <p className="text-slate-500 my-2">Literal text search, not wildcards. Uses cached worksheet values, including staged edits; does not query connected databases. Rerun after edits.</p>
    {error&&<p role="alert" className="text-rose-700">{error}</p>}
    {result&&<div aria-live="polite"><p className="font-semibold my-2">{result.total_matches} matches{result.truncated?' in searched cells':''}</p>
      {result.warnings.map((w,i)=><p key={i} className="text-amber-800 my-1">{w}</p>)}
      <ul>{result.matches.map(m=><li key={`${m.sheet_name}:${m.address}`}><button disabled={busy||pending} onClick={()=>onNavigate(m)} className="text-left border-b py-2 w-full hover:bg-blue-50 disabled:opacity-40"><span className="font-mono text-blue-700">{m.sheet_name}!{m.address}</span><span className="block truncate">{lookIn==='formulas'?(m.raw_input??m.formula??m.display_text):m.display_text}</span>{m.is_staged&&<span>Staged value</span>}</button></li>)}</ul>
      {result.next_offset!==null&&<button disabled={pending||busy} className="border rounded px-2 py-1 mt-2" onClick={()=>void search(result.next_offset!)}>Next 50 matches</button>}
    </div>}
  </aside>;
}
