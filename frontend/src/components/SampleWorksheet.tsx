import {useEffect,useRef,useState} from 'react';
import {api} from '../api';
import {LedgerPanel} from './LedgerPanel';

const INITIAL_SQL='SELECT COUNT(*) AS entries, SUM(amount_cents) AS total_cents\nFROM entries';
const LAST_ROW_SQL='SELECT id, posted, account, amount_cents\nFROM entries ORDER BY id DESC LIMIT 1';

export function SampleWorksheet({onBack}:{onBack:()=>void}){
  const [showSql,setShowSql]=useState(false);
  const [sql,setSql]=useState(INITIAL_SQL);
  const [data,setData]=useState<Awaited<ReturnType<typeof api.sampleQuery>>|null>(null);
  const [error,setError]=useState('');
  const [running,setRunning]=useState(false);
  const active=useRef<AbortController|null>(null);
  const heading=useRef<HTMLHeadingElement|null>(null);
  useEffect(()=>{
    heading.current?.focus();
    return()=>active.current?.abort();
  },[]);
  const run=async()=>{
    if(active.current)return;
    const controller=new AbortController();
    active.current=controller;
    setError('');setData(null);setRunning(true);
    try {
      const result=await api.sampleQuery(sql,controller.signal);
      if(!controller.signal.aborted)setData(result);
    } catch(e) {
      if(!controller.signal.aborted)setError(e instanceof Error?e.message:String(e));
    } finally {
      if(!controller.signal.aborted)setRunning(false);
      active.current=null;
    }
  };
  return <section aria-label="Sample worksheet" className="flex flex-col flex-1 min-h-0 bg-white"
    onKeyDown={e=>{e.stopPropagation();if(e.key==='Escape')onBack();}}>
    <header className="flex items-center flex-wrap gap-3 border-b px-3 py-2">
      <h2 ref={heading} tabIndex={-1} className="font-semibold">Sample worksheet</h2>
      <p className="text-sm text-slate-600 flex-1">Synthetic, read-only data. Not saved in your workbook.</p>
      <button type="button" className="rounded border px-2 py-1" aria-expanded={showSql} onClick={()=>setShowSql(!showSql)}>SQL</button>
      <button type="button" className="rounded border px-2 py-1" onClick={onBack}>Back to workbook</button>
    </header>
    {showSql&&<section aria-label="Sample SQL" className="overflow-auto max-h-[45%] p-3 border-b">
      <form className="grid gap-2" onSubmit={e=>{e.preventDefault();void run();}}>
        <label className="text-sm font-semibold" htmlFor="sample-query">SQL against the sample</label>
        <p className="text-xs">Read-only SQLite SELECT on <code>entries</code> (id, posted, account, amount_cents). Up to 200 result rows.</p>
        <textarea id="sample-query" className="font-mono text-sm border rounded p-2 w-full" rows={3} maxLength={16384} value={sql} onChange={e=>{setSql(e.target.value);}}/>
        <div className="flex gap-2">
          <button className="rounded bg-slate-900 text-white px-2 py-1 disabled:opacity-50" disabled={running||!sql.trim()}>Run query</button>
          <button type="button" className="border rounded px-2 py-1" disabled={running} onClick={()=>setSql(INITIAL_SQL)}>Count and total</button>
          <button type="button" className="border rounded px-2 py-1" disabled={running} onClick={()=>setSql(LAST_ROW_SQL)}>Last entry</button>
        </div>
      </form>
      {running&&<p role="status">Running sample query…</p>}
      {error&&<p role="alert" className="text-red-700">{error}</p>}
      {data&&<div className="max-h-48 overflow-auto mt-2">
        <p role="status" className="text-sm">{data.truncated?'Showing the first 200 result rows.':`${data.rows.length} result rows.`}</p>
        <table className="text-sm border-collapse min-w-full"><thead><tr>{data.columns.map((name,i)=><th key={i} scope="col" className="p-1 text-left border">{name}</th>)}</tr></thead>
        <tbody>{data.rows.map((row,i)=><tr key={i}>{row.map((value,j)=><td key={j} className="p-1 border font-mono">{value??'∅'}</td>)}</tr>)}</tbody></table>
      </div>}
    </section>}
    <LedgerPanel sample onClose={onBack}/>
  </section>;
}
