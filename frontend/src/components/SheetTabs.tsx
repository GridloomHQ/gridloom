import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

interface SheetTabsProps {
  sheets: string[];
  active?: string;
  children?: React.ReactNode;
  busy: boolean;
  onSelect: (name: string) => void;
  onAdd: () => void;
  onRename: (name: string) => void;
  onDelete: (name: string) => void;
}

interface MenuPosition { name: string; x: number; y: number; }

export function SheetTabs({ sheets, active, busy, onSelect, onAdd, onRename, onDelete, children }: SheetTabsProps) {
  const [menu, setMenu] = useState<MenuPosition | null>(null);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const open = (button: HTMLButtonElement, name: string, x: number, y: number) => {
    if (busy) return;
    trigger.current = button;
    setMenu({ name, x, y });
  };
  const close = (restoreFocus = false) => {
    setMenu(null);
    if (restoreFocus) trigger.current?.focus();
  };

  useLayoutEffect(() => {
    if (!menu || !menuRef.current) return;
    const element = menuRef.current;
    const { width, height } = element.getBoundingClientRect();
    const edge = 6;
    element.style.left = `${Math.max(edge, Math.min(menu.x, window.innerWidth - width - edge))}px`;
    element.style.top = `${Math.max(edge, Math.min(menu.y, window.innerHeight - height - edge))}px`;
    element.querySelector<HTMLButtonElement>('button')?.focus();
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const outside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenu(null);
    };
    const resized = () => setMenu(null);
    window.addEventListener('pointerdown', outside);
    window.addEventListener('resize', resized);
    return () => {
      window.removeEventListener('pointerdown', outside);
      window.removeEventListener('resize', resized);
    };
  }, [menu]);

  useEffect(() => {
    if (menu && (busy || !sheets.includes(menu.name))) setMenu(null);
  }, [busy, menu, sheets]);

  return <>
    <div className="flex min-w-0 items-center gap-1 overflow-x-auto" aria-label="Worksheets">
      {sheets.map(name => <button key={name} type="button" disabled={busy}
        onClick={() => { if (name !== active) onSelect(name); }}
        onDoubleClick={() => { if (name === active) onRename(name); }}
        onContextMenu={event => { event.preventDefault(); open(event.currentTarget, name, event.clientX, event.clientY); }}
        onKeyDown={event => {
          if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return;
          event.preventDefault();
          const rect = event.currentTarget.getBoundingClientRect();
          open(event.currentTarget, name, rect.left, rect.top);
        }}
        aria-haspopup="menu" aria-expanded={menu?.name === name}
        aria-current={name === active ? 'page' : undefined}
        title="Right-click for worksheet actions"
        className={`shrink-0 whitespace-nowrap rounded-t px-3 py-1 focus-visible:outline focus-visible:outline-2 focus-visible:outline-emerald-600 ${name === active ? 'border-b-2 border-emerald-600 bg-emerald-50 font-semibold text-emerald-800' : 'hover:bg-slate-100'}`}>
        {name}
      </button>)}
      <button type="button" disabled={busy} onClick={onAdd} aria-label="Add worksheet"
        title="Add worksheet" className="shrink-0 rounded px-2 py-1 font-bold hover:bg-slate-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-emerald-600">+</button>
      {children}
    </div>
    {menu && createPortal(<div ref={menuRef} role="menu" aria-label={`Worksheet actions for ${menu.name}`}
      className="fixed z-[100] min-w-40 rounded-md border border-slate-200 bg-white p-1 text-xs shadow-lg"
      onKeyDown={event => {
        if (event.key === 'Escape') { event.preventDefault(); close(true); return; }
        if (event.key === 'Tab') { close(true); return; }
        if (!['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
        event.preventDefault();
        const options = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
        const current = options.indexOf(document.activeElement as HTMLButtonElement);
        const step = event.key === 'ArrowUp' ? -1 : 1;
        const index = event.key === 'Home' ? 0 : event.key === 'End' ? options.length - 1 : (current + step + options.length) % options.length;
        options[index]?.focus();
      }}>
      <button type="button" role="menuitem" className="block w-full rounded px-3 py-2 text-left hover:bg-slate-100 focus-visible:bg-slate-100"
        onClick={() => { close(true); onRename(menu.name); }}>Rename worksheet</button>
      <button type="button" role="menuitem" disabled={sheets.length <= 1}
        title={sheets.length <= 1 ? 'Keep at least one worksheet' : undefined}
        className="block w-full rounded px-3 py-2 text-left text-red-700 hover:bg-red-50 focus-visible:bg-red-50 disabled:text-slate-400"
        onClick={() => { close(true); onDelete(menu.name); }}>Delete worksheet</button>
    </div>, document.body)}
  </>;
}
