import {version} from '../../package.json';
import {useEffect,useRef,useState} from 'react';
import {api} from '../api';
import type {InvoiceSampleStatus} from '../api';

export function SampleDemo({onClose}:{onClose:()=>void}){
  const [status,setStatus]=useState<InvoiceSampleStatus|null>(null);
  const [pending,setPending]=useState(false);
  const [error,setError]=useState('');
  const closeButton=useRef<HTMLButtonElement|null>(null);
  const command=useRef<AbortController|null>(null);
  const running=status?.state==='generating'||status?.state==='cancelling';
  useEffect(()=>{
    closeButton.current?.focus();
    return()=>command.current?.abort();
  },[]);
  useEffect(()=>{
    if(pending)return;
    const controller=new AbortController();
    let timer:number|undefined;
    const refresh=async()=>{
      try {
        const next=await api.invoiceSampleStatus(controller.signal);
        if(controller.signal.aborted)return;
        setStatus(next);setError('');
        if(next.state!=='generating'&&next.state!=='cancelling')return;
      } catch(e) {
        if(controller.signal.aborted)return;
        setError(e instanceof Error?e.message:String(e));
        if(!running)return;
      }
      timer=window.setTimeout(()=>void refresh(),750);
    };
    void refresh();
    return()=>{controller.abort();window.clearTimeout(timer);};
  },[pending,running]);
  const update=async(action:'start'|'cancel')=>{
    if(command.current)return;
    const controller=new AbortController();
    command.current=controller;
    setPending(true);setError('');
    try {
      const next=await (action==='start'?api.startInvoiceSample:api.cancelInvoiceSample)(controller.signal);
      if(!controller.signal.aborted)setStatus(next);
    } catch(e) {
      if(!controller.signal.aborted)setError(e instanceof Error?e.message:String(e));
    } finally {
      if(!controller.signal.aborted)setPending(false);
      command.current=null;
    }
  };
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
        <p className="text-sm">Add synthetic invoices to the existing demo table until it has 10 million rows. Existing invoices are kept.</p>
        <p className="text-sm text-slate-600">Allow roughly 1 GB of free disk space. This expands <code>SalesDB.invoices</code> on this computer. When generation finishes, choose <strong>Data → Refresh</strong> to see the new rows in the existing live Invoices table. No second table or import is needed.</p>
        {status?.state==='generating'&&<div className="text-sm space-y-1">
          <p role="status">{status.phase??'Generating'} · {(status.rows_written??0).toLocaleString()} of {(status.target_rows??10_000_000).toLocaleString()} rows</p>
          <progress className="w-full" aria-label="Invoice sample progress" max={status.target_rows??10_000_000} value={status.rows_written??0}/>
        </div>}
        {status?.state==='cancelling'&&<p role="status" className="text-sm">Cancelling and rolling back…</p>}
        {status?.state==='cancelled'&&<p role="status" className="text-sm">Generation cancelled. No invoices were added.</p>}
        {status?.state==='failed'&&<p role="alert" className="text-sm text-red-700">{status.message}</p>}
        {error&&<p role="alert" className="text-sm text-red-700">{error}</p>}
        {status?.state==='ready'&&<p role="status" className="bg-slate-100 rounded p-3 text-sm">
          <strong>{status.rows.toLocaleString()} invoices</strong> in the demo database. {status.added.toLocaleString()} added.
        </p>}
        <div className="flex flex-wrap gap-2">
          {running?<button disabled={pending||status?.state==='cancelling'} onClick={()=>void update('cancel')} className="px-3 py-1.5 border rounded disabled:opacity-50">Cancel generation</button>
          :<button disabled={pending||(!status&&!!error)} onClick={()=>void update('start')} className="px-3 py-1.5 bg-slate-900 text-white rounded disabled:opacity-50">Generate demo invoices</button>}
        </div>
      </section>
    </section>
  </div>;
}
