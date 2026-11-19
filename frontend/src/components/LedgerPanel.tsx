import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import {api, LedgerPage, LedgerSpec} from '../api';

import {LEDGER_PAGE_ROWS as PAGE, LEDGER_ROW_PX as ROW, LEDGER_BLOCK_ROWS as WINDOW, ledgerLocation, ledgerOffset, ledgerBlockRows} from './ledgerNavigation';

export function LedgerPanel({onClose}:{onClose:()=>void}) {
  const [connections,C]=useState<string[]>([]), [tables,T]=useState<string[]>([]), [columns,K]=useState<string[]>([]);
  const [connection,SC]=useState(''),[table,ST]=useState(''),[date,SD]=useState(''),[cents,SA]=useState(''),[from,SF]=useState(''),[through,SE]=useState('');
  const [active,A]=useState<LedgerSpec|null>(null),[page,P]=useState<LedgerPage|null>(null),[error,E]=useState(''),[loading,L]=useState(false);
  const [base,B]=useState(0),[row,R]=useState(0),[jump,J]=useState(''),[pageOffset,O]=useState(0);
  const session=useRef<string|null>(null), viewport=useRef<HTMLDivElement|null>(null);
  useEffect(()=>{let live=true;api.getConnections().then(v=>{if(live)C(v.filter(c=>c.kind.toLowerCase()==='sqlite'&&c.path!==':memory:').map(c=>c.name));}).catch(e=>{if(live)E(String(e));});return()=>{live=false;};},[]);
  useEffect(()=>{let live=true;T([]);ST('');if(connection)api.getTables(connection).then(v=>{if(live)T(v);}).catch(e=>{if(live)E(String(e));});return()=>{live=false;};},[connection]);
  useEffect(()=>{let live=true;K([]);SD('');SA('');if(connection&&table)api.getTableSchema(connection,table).then(v=>{if(live)K(v.columns.map(c=>c.name));}).catch(e=>{if(live)E(String(e));});return()=>{live=false;};},[connection,table]);
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
  const open=()=>{session.current=null;P(null);E('');move(0);A({connection,table,date,cents,from:from||null,through:through||null});};
  const select=(name:string,value:string,set:(v:string)=>void,options:string[])=><label className="flex flex-col text-sm">{name}<select className="border rounded p-1" value={value} onChange={e=>set(e.target.value)}><option value="">Choose…</option>{options.map(v=><option key={v}>{v}</option>)}</select></label>;
  const visible=ledgerBlockRows(page?.count??0,base);
  const widths=page?.columns.map(()=>160)??[];
  return <div role="dialog" aria-modal="true" aria-label="SQLite ledger" onKeyDown={e=>{if(e.key==="Escape")onClose();}} className="absolute inset-4 z-50 bg-white border rounded shadow-xl flex flex-col p-4 gap-3 min-h-0">
    <header className="flex justify-between"><h2 className="text-lg font-semibold">Read-only SQLite ledger</h2><button onClick={onClose} className="border rounded px-2">Close</button></header>
    <form className="flex flex-wrap items-end gap-3" onSubmit={e=>{e.preventDefault();open();}}>
      {select('Connection',connection,SC,connections)}{select('Table',table,ST,tables)}
      {select('Posting date (ISO text)',date,SD,columns)}{select('Amount (integer cents)',cents,SA,columns)}
      <label className="flex flex-col text-sm">From<input type="date" value={from} onChange={e=>SF(e.target.value)}/></label>
      <label className="flex flex-col text-sm">Through<input type="date" value={through} onChange={e=>SE(e.target.value)}/></label>
      <button className="border rounded px-2 py-1" disabled={!connection||!table||!date||!cents}>Load / reload</button>
    </form>
    {error && <p role="alert" className="text-red-700">{error}</p>}
    {active && <div className="text-sm flex flex-wrap gap-3 items-center">
      {page && <><span>{page.count.toLocaleString()} matching rows · total: {page.total_cents} cents</span>
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
