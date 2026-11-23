import {version} from '../../package.json';
import {useEffect,useRef} from 'react';

export function AboutDialog({onClose}:{onClose:()=>void}){
  const closeButton=useRef<HTMLButtonElement|null>(null);
  useEffect(()=>{
    const previous=document.activeElement;
    closeButton.current?.focus();
    return()=>{if(previous instanceof HTMLElement)previous.focus();};
  },[]);
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
        <div><h2 id="gridloom-about-title" className="text-lg font-semibold">About Tayriva</h2><p className="text-xs text-slate-600">Version {version}</p></div>
        <button ref={closeButton} onClick={onClose} className="px-2 py-1 border rounded">Close</button>
      </header>
    </section>
  </div>;
}
