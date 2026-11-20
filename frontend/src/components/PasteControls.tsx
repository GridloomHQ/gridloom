import {useEffect, useRef, useState} from 'react';

export type PasteMode = 'values' | 'formulas';

export function PasteControls({open, onOpen, onClose, onPaste, busy, copySource}: {
  open: boolean; onOpen: () => void; onClose: () => void; onPaste: (mode: PasteMode) => void;
  busy: boolean; copySource: string | null;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const [help, setHelp] = useState(false);
  useEffect(() => {
    if (!open) return;
    setHelp(false);
    const origin = document.activeElement;
    panel.current?.focus();
    return () => { if (origin instanceof HTMLElement && origin.isConnected) origin.focus(); };
  }, [open]);
  return <div className="relative flex items-center gap-2">
    <button type="button" className="border rounded px-2 py-1" disabled={busy}
      aria-expanded={open} aria-keyshortcuts="Control+Alt+V Meta+Alt+V" onClick={open ? onClose : onOpen} title="Ctrl/Command + Alt/Option + V">Paste Special</button>
    <button type="button" className="border rounded px-2 py-1" aria-expanded={help}
      onClick={() => setHelp(!help)}>Keyboard shortcuts</button>
    {open && <div ref={panel} tabIndex={-1} role="group" aria-label="Paste Special options"
      onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } }}
      className="absolute left-0 top-full z-40 w-80 rounded border bg-white p-3 shadow">
      <p className="mb-2">{copySource
        ? `Last worksheet copy: ${copySource}.`
        : 'Copy cells in Gridloom first. System clipboard text uses ordinary Paste.'}</p>
      <div className="flex gap-2">
        <button type="button" className="border rounded px-2 py-1" disabled={busy || !copySource}
          onClick={() => { onClose(); onPaste('values'); }}>Values only</button>
        <button type="button" className="border rounded px-2 py-1" disabled={busy || !copySource}
          onClick={() => { onClose(); onPaste('formulas'); }}>Formulas only</button>
        <button type="button" className="border rounded px-2 py-1" onClick={onClose}>Close</button>
      </div>
      <p className="mt-2 text-slate-600">Destination formatting stays. Formulas only skips source constants and blanks.</p>
    </div>}
    {help && <div role="region" aria-label="Worksheet keyboard shortcuts"
      onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); setHelp(false); } }}
      className="absolute left-0 top-full z-30 w-80 rounded border bg-white p-3 shadow">
      <p className="mb-2 font-semibold">Use Ctrl on Windows/Linux or Command on Mac.</p>
      <dl className="grid grid-cols-[1fr_auto] gap-1">
        <dt>Copy / paste</dt><dd>Ctrl+C / Ctrl+V</dd>
        <dt>Paste Special</dt><dd>Ctrl+Alt+V</dd>
        <dt>Undo / redo</dt><dd>Ctrl+Z / Ctrl+Shift+Z</dd>
        <dt>Fill down / right</dt><dd>Ctrl+D / Ctrl+R</dd>
        <dt>Find</dt><dd>Ctrl+F</dd>
        <dt>Save / Save As</dt><dd>Ctrl+S / Ctrl+Shift+S</dd>
        <dt>Edit a cell</dt><dd>F2</dd>
        <dt>Cycle formula reference locks</dt><dd>F4 in editor</dd>
        <dt>Extend selection</dt><dd>Shift+arrow</dd>
        <dt>First cell</dt><dd>Ctrl+Home</dd>
        <dt>Jump to data edge</dt><dd>Ctrl+Arrow (Shift extends selection)</dd>
        <dt>Select column</dt><dd>Ctrl+Space</dd>
        <dt>Select row</dt><dd>Shift+Space</dd>
      </dl>
      <p className="mt-2 text-slate-600">Alt is Option on Mac.</p>
      <button type="button" className="mt-2 border rounded px-2 py-1" onClick={() => setHelp(false)}>Close shortcuts</button>
    </div>}
  </div>;
}
