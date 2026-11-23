import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import {api} from '../api';
import type {LedgerPage, LedgerSpec, LedgerTarget, LedgerTrace} from '../api';
import {coordToA1} from '../utils/coords';
import {compareLedgerBalance,sameLedgerSpec} from './ledgerTrace';

import {LEDGER_PAGE_ROWS as PAGE, LEDGER_ROW_PX as ROW, LEDGER_BLOCK_ROWS as WINDOW, ledgerLocation, ledgerOffset, ledgerBlockRows} from './ledgerNavigation';

export function LedgerPanel({onClose,target}:{onClose:()=>void;target?:LedgerTarget}) {
  const [connections,C]=useState<string[]>([]), [tables,T]=useState<string[]>([]), [columns,K]=useState<string[]>([]);
  const [connection,SC]=useState(''),[table,ST]=useState(''),[date,SD]=useState(''),[cents,SA]=useState(''),[from,SF]=useState(''),[through,SE]=useState('');
  const [accountColumn,setAccountColumn]=useState(''), [account,setAccount]=useState('');
  const [trace,setTrace]=useState<LedgerTrace|null>(null),[traceError,setTraceError]=useState(''),[saving,setSaving]=useState(false);
  const [balance,setBalance]=useState<string|null>(null);
  const traceRequest=useRef<AbortController|null>(null);
  useEffect(()=>()=>traceRequest.current?.abort(),[]);
  const [active,A]=useState<LedgerSpec|null>(null),[page,P]=useState<LedgerPage|null>(null),[error,E]=useState(''),[loading,L]=useState(false);
  const [base,B]=useState(0),[row,R]=useState(0),[jump,J]=useState(''),[pageOffset,O]=useState(0);
  const session=useRef<string|null>(null), viewport=useRef<HTMLDivElement|null>(null);
  useEffect(()=>{let live=true;api.getConnections().then(v=>{if(live)C(v.filter(c=>c.kind.toLowerCase()==='sqlite'&&c.path!==':memory:').map(c=>c.name));}).catch(e=>{if(live)E(String(e));});return()=>{live=false;};},[]);
  useEffect(()=>{let live=true;T([]);if(connection)api.getTables(connection).then(v=>{if(live)T(v);}).catch(e=>{if(live)E(String(e));});return()=>{live=false;};},[connection]);
  useEffect(()=>{let live=true;K([]);if(connection&&table)api.getTableSchema(connection,table).then(v=>{if(live)K(v.columns.map(c=>c.name));}).catch(e=>{if(live)E(String(e));});return()=>{live=false;};},[connection,table]);
  const offset=ledgerOffset(base,row);
  useEffect(()=>{
    if(!active || error) return;
    const controller=new AbortController();
    L(true);
    const timer=setTimeout(()=>{void api.ledgerPage({spec:active,session:session.current,offset,limit:PAGE},controller.signal)
      .then(v=>{if(!controller.signal.aborted){session.current=v.session;P(v);O(offset);L(false);}})
      .catch(e=>{if(!controller.signal.aborted){P(null);E(String(e));L(false);}});},session.current?65:0);
    return()=>{clearTimeout(timer);controller.abort();};
  },[active,offset,error]);
  const pendingScroll=useRef<number|null>(null);
  useLayoutEffect(()=>{if(pendingScroll.current!==null && viewport.current){viewport.current.scrollTop=pendingScroll.current;pendingScroll.current=null;} });
  const move=(to:number)=>{
    const {base:b,row:r}=ledgerLocation(to);
    B(b);R(r);
    pendingScroll.current=r*ROW;
  };
  const presentedTarget=target ? `${target.sheet_name}!${coordToA1(target.row,target.col)}` : '';
  const draft: LedgerSpec={connection,table,date,cents,from:from||null,through:through||null,account:accountColumn&&account.length ? {column:accountColumn,value:account} : null};
  const openSpec=(spec:LedgerSpec,capturedBalance:string|null)=>{
    session.current=null;P(null);E('');move(0);setBalance(capturedBalance);A({...spec});
  };
  useEffect(()=>{
    if (!target) return;
    const controller=new AbortController();
    traceRequest.current=controller;
    void api.getLedgerTrace(target,controller.signal).then(v=>{
      if(controller.signal.aborted)return;
      setTrace(v);
      if(v.link){
        const s=v.link;
        SC(s.connection);ST(s.table);SD(s.date);SA(s.cents);SF(s.from??'');SE(s.through??'');
        setAccountColumn(s.account?.column??'');setAccount(s.account?.value??'');
        if(!v.source_changed)openSpec(s,v.balance_cents);
      }
    }).catch(e=>{if(!controller.signal.aborted)setTraceError(String(e));});
    return()=>controller.abort();
    // The target is fixed for the lifetime of this panel.
  },[target]);
  const open=async()=>{
    if(!target){openSpec(draft,null);return;}
    traceRequest.current?.abort();
    const controller=new AbortController();traceRequest.current=controller;
    setTraceError('');setSaving(true);
    // Drop old totals immediately: never show a comparison for the wrong query.
    A(null);P(null);setBalance(null);E('');
    try {
      const fresh=await api.getLedgerTrace(target,controller.signal);
      if(!controller.signal.aborted){setTrace(fresh);openSpec(draft,fresh.balance_cents);}
    } catch(e) {if(!controller.signal.aborted)setTraceError(String(e));}
    finally {if(!controller.signal.aborted)setSaving(false);}
  };
  const storeLink=async(remove:boolean)=>{
    if(!target || (!remove && !session.current))return;
    traceRequest.current?.abort();
    const controller=new AbortController();traceRequest.current=controller;
    setTraceError('');setSaving(true);
    try {
      const v=await api.setLedgerTrace(target,remove?null:session.current,controller.signal);
      if(!controller.signal.aborted){setTrace(v);setBalance(v.balance_cents);}
    } catch(e) {
      if(!controller.signal.aborted){
        setTraceError(String(e));
        if(!remove){P(null);setBalance(null);}
      }
    } finally {if(!controller.signal.aborted)setSaving(false);}
  };
  const comparison=page&&balance!==null ? compareLedgerBalance(balance,page.total_cents,page.count) : null;
  const isSaved=!!active&&sameLedgerSpec(active,trace?.link??null)&&!trace?.source_changed;
  const canSave=!!target&&!!active&&!!page&&!error&&balance!==null&&sameLedgerSpec(active,draft)
    &&!!active.account&&!!active.from&&!!active.through&&!isSaved&&!loading&&!saving;
  const changeTable=(value:string)=>{ST(value);SD('');SA('');setAccountColumn('');setAccount('');};
  const changeConnection=(value:string)=>{SC(value);changeTable('');};
  const select=(name:string,value:string,set:(v:string)=>void,options:string[])=><label className="flex flex-col text-sm">{name}<select aria-label={name} className="border rounded p-1" value={value} onChange={e=>set(e.target.value)}><option value="">Choose…</option>{options.map(v=><option key={v}>{v}</option>)}</select></label>;
  const visible=ledgerBlockRows(page?.count??0,base);
  const widths=page?.columns.map(()=>160)??[];
  return <div role="dialog" aria-modal="true" aria-label="SQLite ledger" onKeyDown={e=>{if(e.key==="Escape"){e.stopPropagation();onClose();}}} className="absolute inset-4 z-50 bg-white border rounded shadow-xl flex flex-col p-4 gap-3 min-h-0">
    <header className="flex justify-between"><h2 className="text-lg font-semibold">{target ? `Trace balance: ${presentedTarget}` : "Read-only SQLite ledger"}</h2><button onClick={onClose} className="border rounded px-2">Close</button></header>
    <form className="flex flex-wrap items-end gap-3" onSubmit={e=>{e.preventDefault();void open();}}>
      {select('Connection',connection,changeConnection,connections)}{select('Table',table,changeTable,tables)}
      {select('Posting date (ISO text)',date,SD,columns)}{select('Amount (integer cents)',cents,SA,columns)}
      {select('Account column',accountColumn,value=>{setAccountColumn(value);if(!value)setAccount('');},columns)}
      <label className="flex flex-col text-sm">Account code<input className="border rounded p-1" maxLength={1024} value={account} disabled={!accountColumn} onChange={e=>setAccount(e.target.value)}/></label>
      <label className="flex flex-col text-sm">From<input type="date" value={from} onChange={e=>SF(e.target.value)}/></label>
      <label className="flex flex-col text-sm">Through<input type="date" value={through} onChange={e=>SE(e.target.value)}/></label>
      <button className="border rounded px-2 py-1" disabled={!connection||!table||!date||!cents||(!!accountColumn!==!!account.length)||saving||(!!target&&!trace&&!traceError)}>Load / reload</button>
    </form>
    {target&&<div className="text-sm space-y-2">
      <p>Compares the selected cell in currency units with the sum of matching ledger entries in integer cents. Reload to read the current cell and source again.</p>
      {trace?.source_changed&&<p role="alert" className="text-amber-800">The saved connection has changed. Review the replacement source, then load and save a new link.</p>}
      {trace&&trace.balance_cents===null&&<p>This cell is not a current, exact numeric balance. Recalculate or commit your edit first; fractional cents and floating-point values are not compared.</p>}
      {comparison&&<p aria-live="polite">
        <strong>{comparison.status}</strong> · Cell at load: {comparison.balance} · Ledger: {comparison.total} · Difference (cell − ledger): {comparison.difference}
      </p>}
      {trace?.link&&<p>Saved link: {trace.link.connection} / {trace.link.table} · {trace.link.account?.column} = {trace.link.account?.value} · {trace.link.from} to {trace.link.through}</p>}
      <div className="flex gap-2">
        <button type="button" disabled={!canSave} onClick={()=>void storeLink(false)} className="border rounded px-2 py-1">Save link to cell</button>
        {trace?.link&&<button type="button" disabled={saving} onClick={()=>void storeLink(true)} className="border rounded px-2 py-1">Remove saved link</button>}
      </div>
      <p className="text-slate-600">To reuse this link after closing Tayriva, save the workbook. Saving a link does not change the source database.</p>
      {traceError&&<p role="alert" className="text-red-700">{traceError}</p>}
    </div>}
    {error && <p role="alert" className="text-red-700">{error}</p>}
    {active && <div className="text-sm flex flex-wrap gap-3 items-center">
      {page && <>{!sameLedgerSpec(active,draft)&&<strong>Showing loaded results; reload to apply your changes.</strong>}
        <span>{page.count.toLocaleString()} matching rows · total: {page.total_cents} cents</span>
        <button className="border px-2" disabled={base===0} onClick={()=>move(base-WINDOW)}>Previous block</button>
        <button className="border px-2" disabled={base+WINDOW>=page.count} onClick={()=>move(base+WINDOW)}>Next block</button>
        <form onSubmit={e=>{e.preventDefault();const n=Number(jump);if(Number.isSafeInteger(n)&&n>=1&&n<=page.count)move(n-1);}}><label>Go to row <input className="border w-24 px-1" type="number" min={1} max={page.count} value={jump} onChange={e=>J(e.target.value)}/></label> <button className="border px-2">Go</button></form>
        {visible>0&&<span>Block {(base+1).toLocaleString()}–{(base+visible).toLocaleString()}</span>}</>}
      {loading && <span role="status">Loading…</span>}
    </div>}
    <div ref={viewport} onScroll={e=>R(Math.floor(e.currentTarget.scrollTop/ROW))} className="flex-1 min-h-0 overflow-auto border">
      {page && <div style={{minWidth:64+widths.reduce((a,b)=>a+b,0)}}>
        <div className="sticky top-0 z-10 flex bg-slate-100 font-semibold text-xs" style={{height:ROW}}>
          <div className="w-16 shrink-0 p-1">Row</div>{page.columns.map((c,i)=><div key={c} style={{width:widths[i]}} className="shrink-0 truncate p-1 border-l" title={c}>{c}</div>)}
        </div>
        <div className="relative" style={{height:visible*ROW}}>
          {page.rows.map((cells,i)=>{const r=pageOffset+i;if(r<base||r>=base+visible)return null;return <div key={r} className="absolute flex text-xs border-b w-full" style={{height:ROW,top:(r-base)*ROW}}>
            <div className="w-16 shrink-0 p-1">{r+1}</div>{cells.map((c,j)=><div key={j} className="shrink-0 truncate p-1 border-l" title={c??undefined} style={{width:widths[j]}}>{c??'∅'}</div>)}
          </div>;})}
        </div></div>}
    </div>
  </div>;
}
