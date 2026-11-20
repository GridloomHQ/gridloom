import {version} from '../../package.json';
import {useEffect,useRef,useState} from 'react';
import {api} from '../api';
import type {SampleStatus} from '../api';
import {LedgerPanel} from './LedgerPanel';

const ms=(value:number)=>value>=1000?`${(value/1000).toFixed(2)} s`:`${value.toFixed(1)} ms`;
function cents(value:string){
  const amount=BigInt(value);
  const whole=amount<0n?-amount:amount;
  return `${amount<0n?'−':''}${(whole/100n).toLocaleString()}.${(whole%100n).toString().padStart(2,'0')}`;
}

export function SampleDemo({onClose}:{onClose:()=>void}){
  const [status,setStatus]=useState<SampleStatus|null>(null);
  const [pending,setPending]=useState(false);
  const [error,setError]=useState('');
  const [ledger,setLedger]=useState(false);
  const closeButton=useRef<HTMLButtonElement|null>(null);
  const command=useRef<AbortController|null>(null);
  const busy=status?.state==='generating'||status?.state==='cancelling';
  useEffect(()=>()=>{
    command.current?.abort();
  },[]);
  useEffect(()=>{
    if(ledger)return;
    closeButton.current?.focus();
  },[ledger]);
  useEffect(()=>{
    if(pending||ledger)return;
    const controller=new AbortController();
    const refresh=async()=>{
      try {
        const next=await api.sampleStatus(controller.signal);
        if(controller.signal.aborted)return;
        setStatus(next);setError('');
        if(next.state!=='generating'&&next.state!=='cancelling')return;
      }catch(e){
        if(controller.signal.aborted)return;
        setError(String(e));
        if(!busy)return;
      }
      timer=window.setTimeout(()=>void refresh(),750);
    };
    let timer=window.setTimeout(()=>void refresh(),0);
    return()=>{controller.abort();window.clearTimeout(timer);};
  },[pending,busy,ledger]);
  const update=async(action:'start'|'cancel')=>{
    if(command.current)return;
    const controller=new AbortController();
    command.current=controller;
    setPending(true);setError('');
    try {
      const next=await (action==='start'?api.startSample:api.cancelSample)(controller.signal);
      if(!controller.signal.aborted)setStatus(next);
    } catch(e) {
      if(!controller.signal.aborted)setError(String(e));
    } finally {
      if(!controller.signal.aborted)setPending(false);
      command.current=null;
    }
  };
  if(ledger)return <LedgerPanel sample onClose={()=>setLedger(false)}/>;
  return <div className="fixed inset-0 z-50 flex justify-center items-center bg-black/30 p-4" onKeyDown={e=>{
    e.stopPropagation();
    if(e.key==='Escape')onClose();
    if(e.key==='Tab'){
      const focusable=Array.from(e.currentTarget.querySelectorAll<HTMLElement>('button:not([disabled])'));
      const first=focusable[0],last=focusable[focusable.length-1];
      if(e.shiftKey&&document.activeElement===first){e.preventDefault();last?.focus();}
      if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first?.focus();}
    }
  }}>
    <section role="dialog" aria-modal="true" aria-labelledby="gridloom-about-title" className="bg-white rounded-lg shadow-xl max-w-xl w-full max-h-full overflow-y-auto p-5 space-y-4">
      <header className="flex items-center justify-between gap-4">
        <div><h2 id="gridloom-about-title" className="text-lg font-semibold">About Gridloom</h2><p className="text-xs text-slate-600">Version {version}</p></div>
        <button ref={closeButton} onClick={onClose} className="px-2 py-1 border rounded">Close</button>
      </header>
      <section aria-labelledby="gridloom-sample-title" className="border-t pt-4 space-y-3">
        <h3 id="gridloom-sample-title" className="font-semibold">Sample</h3>
        <p className="text-sm">Explore 10 million synthetic journal entries. SQLite keeps the rows on disk; Gridloom reads only the pages you browse.</p>
        <p className="text-sm text-slate-600">Allow about 1 GB of free disk space. The sample stays on this computer and won’t change your workbook.</p>
        {status?.state==='generating'&&<div className="text-sm space-y-1">
          <p role="status">{status.phase} · {status.rows_written.toLocaleString()} of {status.target_rows.toLocaleString()} rows</p>
          <progress className="w-full" aria-label="Sample rows written" max={status.target_rows} value={status.rows_written}/>
        </div>}
        {status?.state==='cancelling'&&<p role="status" className="text-sm">Cancelling after the current SQLite step…</p>}
        {status?.state==='cancelled'&&<p role="status" className="text-sm">Sample generation cancelled. No finished sample was saved.</p>}
        {status?.state==='failed'&&<p role="alert" className="text-sm text-red-700">{status.message}</p>}
        {error&&<p role="alert" className="text-sm text-red-700">{error}</p>}
        {status?.state==='ready'&&<div className="bg-slate-100 rounded p-3 text-sm space-y-1">
          <p><strong>{status.stats.rows.toLocaleString()} rows</strong> · {(status.stats.bytes/1024**2).toFixed(0)} MiB on disk</p>
          <p>{status.stats.generation_ms===null?'Reused the local sample':`Generated and indexed in ${ms(status.stats.generation_ms)}`}</p>
          <p>Count and sum: {ms(status.stats.totals_ms)} · total {cents(status.stats.total_cents)}</p>
          <p>Indexed filter (account 1100, Jan 1–7): {ms(status.stats.filter_ms)} · {status.stats.filtered_rows.toLocaleString()} rows · total {cents(status.stats.filtered_cents)}</p>
          <p>Indexed lookup of last entry: {ms(status.stats.seek_ms)}</p>
          <p className="text-xs text-slate-600">SQLite timings measured on this computer. Not a comparison with Excel.</p>
        </div>}
        <div className="flex flex-wrap gap-2">
          {status?.state==='ready'?<button onClick={()=>setLedger(true)} className="px-3 py-1.5 bg-slate-900 text-white rounded">Browse sample ledger</button>
          :busy?<button disabled={pending||status?.state==='cancelling'} onClick={()=>void update('cancel')} className="px-3 py-1.5 border rounded disabled:opacity-50">Cancel generation</button>
          :<button disabled={pending} onClick={()=>void update('start')} className="px-3 py-1.5 bg-slate-900 text-white rounded disabled:opacity-50">Generate 10-million-row sample</button>}
        </div>
      </section>
    </section>
  </div>;
}
