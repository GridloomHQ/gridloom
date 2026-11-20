import { useState, type ReactNode } from 'react';

interface ToolbarProps {
  busy: boolean;
  onUndo: () => void; onRedo: () => void; onFillDown: () => void; onFillRight: () => void;
  closeAddon: boolean; onToggleCloseAddon: (enabled: boolean) => void; onOpenClose: () => void;
  onOpenExplain: () => void; onOpenQuestions: () => void;
  onOpenSchemaBrowser: () => void; onOpenQueryEditor: () => void; onOpenInsertTable: () => void;
  onOpenStagedReview: () => void; onOpenPivots: () => void; onOpenCharts: () => void;
  onOpenAuditLog: () => void; onOpenBenchmarks: () => void;
  onNewWorkbook: () => void; onSaveWorkbook: () => void; onSaveAsWorkbook: () => void;
  onOpenWorkbook: () => void; onRefreshAll: () => void; onApplyFormat: (format: string) => void;
  stagedCount: number; onOpenAbout?: () => void;
  fileExtras: ReactNode; homeExtras: ReactNode; insertExtras: ReactNode; dataExtras: ReactNode; viewExtras: ReactNode;
}

function Group({ name, children }: {name: string; children: ReactNode}) {
  return <section aria-label={name} className="border-r border-slate-200 px-2 py-1 last:border-r-0">
    <h2 className="px-1 pb-1 text-[10px] font-semibold uppercase tracking-wider text-slate-500">{name}</h2>
    <div className="flex flex-wrap items-center gap-1">{children}</div>
  </section>;
}

export function Toolbar(p: ToolbarProps) {
  const [addonsOpen, setAddonsOpen] = useState(false);
  const command = (label: string, run: () => void, title?: string) => <button type="button" disabled={p.busy}
    onMouseDown={e => e.preventDefault()} onClick={run} title={title}
    className="rounded px-2 py-1 text-slate-800 hover:bg-slate-100 focus-visible:outline-indigo-500 disabled:opacity-50">{label}</button>;
  return <div role="region" aria-label="Worksheet commands" className="border-b border-slate-200 bg-white px-2 py-1 shadow-sm text-xs select-none">
    <div className="flex flex-wrap items-stretch gap-y-1">
      <div className="self-center px-2 font-bold text-emerald-800">Gridloom</div>
      <Group name="File">
        {command('New', p.onNewWorkbook, 'New Workbook')}{command('Open', p.onOpenWorkbook, 'Open Workbook (.gridloom)')}
        {command('Save', p.onSaveWorkbook, 'Save Workbook')}{command('Save As', p.onSaveAsWorkbook)}{p.fileExtras}
      </Group>
      <Group name="Home">
        {command('Undo', p.onUndo, 'Undo worksheet edit (Ctrl/Cmd+Z)')}{command('Redo', p.onRedo)}
        {command('Fill down', p.onFillDown, 'Fill selected range from its first row (Ctrl/Cmd+D)')}
        {command('Fill right', p.onFillRight)}{p.homeExtras}
      </Group>
      <Group name="Format">
        {command('123', () => p.onApplyFormat('General'), 'Format as General')}
        {command('$', () => p.onApplyFormat('Currency'), 'Format as Currency ($)')}
        {command('%', () => p.onApplyFormat('Percent'), 'Format as Percent (%)')}
        {command('B', () => p.onApplyFormat('Bold'), 'Toggle bold')}
        {command('I', () => p.onApplyFormat('Italic'), 'Toggle italic')}
        <label className="flex gap-1 items-center">Fill <input type="color" aria-label="Cell fill color" disabled={p.busy}
          defaultValue="#fef3c7" onChange={e => p.onApplyFormat(`Fill:${e.target.value}`)} className="w-7 h-6"/></label>
        {command('No fill', () => p.onApplyFormat('Fill:'), 'Clear cell fill')}
      </Group>
      <Group name="Insert">
        {p.insertExtras}{command('Insert Table', p.onOpenInsertTable)}{command('Pivot', p.onOpenPivots)}{command('Charts', p.onOpenCharts)}
      </Group>
      <Group name="Data">
        {command('Databases', p.onOpenSchemaBrowser)}{command('SQL Query', p.onOpenQueryEditor)}
        {command('Refresh', p.onRefreshAll, 'Refresh All Data Sources')}{p.dataExtras}
      </Group>
      <Group name="Review">
        {command('Explain & Review', p.onOpenExplain)}{command('Open questions', p.onOpenQuestions)}
        <button type="button" disabled={p.busy} onClick={p.onOpenStagedReview}
          className={p.stagedCount ? 'rounded bg-amber-100 px-2 py-1 text-amber-900 font-semibold' : 'rounded px-2 py-1 hover:bg-slate-100'}>
          Review Edits <span className="tabular-nums">{p.stagedCount}</span>
        </button>
        {command('Audit', p.onOpenAuditLog, 'Audit Log of Applied Writebacks')}
      </Group>
      <Group name="View">{p.viewExtras}</Group>
      <Group name="Help">
        {p.onOpenAbout && command('About', p.onOpenAbout)}
        {command('Benchmarks', p.onOpenBenchmarks, 'Performance Acceptance Protocol')}
        <button type="button" disabled={p.busy} aria-expanded={addonsOpen} aria-controls="worksheet-addons"
          onClick={() => setAddonsOpen(open => !open)} className="rounded px-2 py-1 hover:bg-slate-100">Add-ons</button>
        {p.closeAddon && command('Monthly close', p.onOpenClose)}
      </Group>
    </div>
    {addonsOpen && <div id="worksheet-addons" className="flex flex-wrap items-center gap-2 border-t px-2 py-2">
      <label className="flex items-center gap-2"><input type="checkbox" checked={p.closeAddon} disabled={p.busy}
        onChange={e => p.onToggleCloseAddon(e.target.checked)} />Enable Monthly close add-on</label>
      <span className="text-slate-600">Adds trial-balance and close reports. Switching it off does not remove any workbook data.</span>
    </div>}
  </div>;
}
