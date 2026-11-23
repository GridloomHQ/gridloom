import {useId, useLayoutEffect, useRef, type ReactNode} from 'react';
import {worksheetCommandClass} from './Toolbar';

export type PasteMode = 'values' | 'formulas';

export function PasteSpecialDialog({onClose, onPaste, busy, copySource}: {
  onClose: () => void; onPaste: (mode: PasteMode) => void;
  busy: boolean; copySource: string | null;
}) {
  return <WorksheetDialog title="Paste Special" onClose={onClose}>
    <div role="group" aria-label="Paste Special options">
      <p className="mb-2">{copySource
        ? `Last worksheet copy: ${copySource}.`
        : 'Copy cells in Tayriva first. System clipboard text uses ordinary Paste.'}</p>
      <div className="flex gap-2">
        <button type="button" className={worksheetCommandClass} disabled={busy || !copySource}
          onClick={() => { onClose(); onPaste('values'); }}>Values only</button>
        <button type="button" className={worksheetCommandClass} disabled={busy || !copySource}
          onClick={() => { onClose(); onPaste('formulas'); }}>Formulas only</button>
        <button type="button" className={worksheetCommandClass} onClick={onClose}>Close</button>
      </div>
      <p className="mt-2 text-slate-600">Destination formatting stays. Formulas only skips source constants and blanks.</p>
    </div>
  </WorksheetDialog>;
}

function WorksheetDialog({title, children, onClose}: {
  title: string; children: ReactNode; onClose: () => void;
}) {
  const headingId = useId();
  const ref = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => {
    const dialog = ref.current;
    const origin = document.activeElement;
    dialog?.showModal();
    return () => {
      dialog?.close();
      if (origin instanceof HTMLElement && origin.isConnected) origin.focus();
    };
  }, []);
  return <dialog ref={ref} aria-labelledby={headingId}
    onCancel={event => {event.preventDefault(); onClose();}}
    onKeyDown={event => {
      event.stopPropagation();
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') event.preventDefault();
    }}
    className="max-h-[calc(100dvh-2rem)] w-96 max-w-[calc(100%-2rem)] overflow-y-auto rounded-lg border border-slate-300 bg-white p-4 text-sm text-slate-900 shadow-xl backdrop:bg-slate-950/30">
    <h2 id={headingId} className="mb-3 text-base font-semibold">{title}</h2>
    {children}
  </dialog>;
}

export function KeyboardShortcutsDialog({onClose}: {onClose: () => void}) {
  return <WorksheetDialog title="Worksheet keyboard shortcuts" onClose={onClose}>
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
      <button type="button" className={`mt-3 ${worksheetCommandClass}`} autoFocus onClick={onClose}>Close shortcuts</button>
  </WorksheetDialog>;
}
