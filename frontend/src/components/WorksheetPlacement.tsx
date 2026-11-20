import { useId } from 'react';
import { coordToA1 } from '../utils/coords';
import { parsePlacement, type GridPlacement, type PlacementDraft } from '../utils/worksheetPlacement';

export function WorksheetPlacement({ sheets, selection, draft, onChange, disabled }: {
  sheets: readonly string[];
  selection: GridPlacement;
  draft: PlacementDraft;
  onChange: (next: PlacementDraft) => void;
  disabled?: boolean;
}) {
  const id = useId();
  const invalidAnchor = !!draft.anchor.trim() && !parsePlacement(draft, [draft.sheetName]);
  const selectionLabel = coordToA1(selection.row, selection.col);
  return <fieldset disabled={disabled} className="flex flex-wrap items-end gap-2 text-xs">
    <legend className="text-xs font-bold text-slate-700 mb-1">Worksheet placement</legend>
    <label htmlFor={id + '-sheet'} className="flex flex-col gap-1 font-medium">
      Worksheet
      <select id={id + '-sheet'} value={draft.sheetName} onChange={e => onChange({...draft, sheetName: e.target.value})}
        className="px-2 py-1 border border-slate-300 rounded bg-white max-w-44">
        <option value="">Choose a worksheet</option>
        {sheets.map(sheet => <option key={sheet} value={sheet}>{sheet}</option>)}
      </select>
    </label>
    <label htmlFor={id + '-anchor'} className="flex flex-col gap-1 font-medium">
      Anchor cell
      <input id={id + '-anchor'} value={draft.anchor} onChange={e => onChange({...draft, anchor: e.target.value})}
        aria-invalid={invalidAnchor} aria-describedby={id + '-help'} autoComplete="off" spellCheck={false}
        placeholder="e.g. A1" className="px-2 py-1 border border-slate-300 rounded w-24 font-mono" />
    </label>
    <button type="button" onClick={() => onChange({sheetName: selection.sheetName, anchor: selectionLabel})}
      className="px-2 py-1 border border-slate-300 rounded text-slate-700 hover:bg-slate-100 disabled:opacity-50">
      Use selection ({selectionLabel})
    </button>
    <p id={id + '-help'} className={`basis-full ${invalidAnchor ? 'text-rose-700' : 'text-slate-500'}`}>
      {invalidAnchor ? 'Enter a valid worksheet cell, such as B4.' : 'Choose where the top-left header cell will go.'}
    </p>
  </fieldset>;
}
