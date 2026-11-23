import { useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';

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
  stagedCount: number; onOpenAbout: () => void;
  fileExtras: ReactNode; homeExtras: ReactNode; insertExtras: ReactNode; dataExtras: ReactNode; viewExtras: ReactNode;
}

const tabs = ['File', 'Home', 'Format', 'Insert', 'Data', 'Review', 'View', 'Add-ons', 'Help'] as const;
type Tab = (typeof tabs)[number];

// Commands use a visible border even at rest; static labels remain plain text.
export const worksheetCommandClass = 'shrink-0 rounded border border-slate-300 bg-white px-2 py-1 font-medium text-slate-800 hover:border-slate-400 hover:bg-slate-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-indigo-600 disabled:opacity-50 disabled:hover:border-slate-300 disabled:hover:bg-white';

export function Toolbar(p: ToolbarProps) {
  const id = useId();
  const [active, setActive] = useState<Tab>('Home');
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const activePanel = useRef<HTMLDivElement>(null);
  const activate = (tab: Tab) => {
    if (tab !== active) {
      if (activePanel.current) activePanel.current.scrollLeft = 0;
      setActive(tab);
    }
  };
  const navigate = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    let next: number;
    switch (event.key) {
      case 'ArrowRight': next = (index + 1) % tabs.length; break;
      case 'ArrowLeft': next = (index + tabs.length - 1) % tabs.length; break;
      case 'Home': next = 0; break;
      case 'End': next = tabs.length - 1; break;
      default: return;
    }
    event.preventDefault();
    activate(tabs[next]);
    tabRefs.current[next]?.focus();
  };
  const command = (label: string, run: () => void, title?: string, accessibleLabel?: string) => <button type="button" disabled={p.busy}
    onMouseDown={e => e.preventDefault()} onClick={run} title={title} aria-label={accessibleLabel}
    className={worksheetCommandClass}>{label}</button>;
  const commands: Record<Tab, ReactNode> = {
      File: <>
        {command('New', p.onNewWorkbook, 'New Workbook')}{command('Open', p.onOpenWorkbook, 'Open Workbook (.tay)')}
        {command('Save', p.onSaveWorkbook, 'Save Workbook')}{command('Save As', p.onSaveAsWorkbook)}{p.fileExtras}
      </>,
      Home: <>
        {command('Undo', p.onUndo, 'Undo worksheet edit (Ctrl/Cmd+Z)')}{command('Redo', p.onRedo)}
        {command('Fill down', p.onFillDown, 'Fill selected range from its first row (Ctrl/Cmd+D)')}
        {command('Fill right', p.onFillRight)}{p.homeExtras}
      </>,
      Format: <>
        {command('123', () => p.onApplyFormat('General'), 'Format as General', 'Format as General')}
        {command('$', () => p.onApplyFormat('Currency'), 'Format as Currency ($)', 'Format as Currency ($)')}
        {command('%', () => p.onApplyFormat('Percent'), 'Format as Percent (%)', 'Format as Percent (%)')}
        {command('B', () => p.onApplyFormat('Bold'), 'Toggle bold', 'Toggle bold')}
        {command('I', () => p.onApplyFormat('Italic'), 'Toggle italic', 'Toggle italic')}
        <label className="flex gap-1 items-center rounded border border-slate-300 px-2">Fill <input type="color" aria-label="Cell fill color" disabled={p.busy}
          defaultValue="#fef3c7" onChange={e => p.onApplyFormat(`Fill:${e.target.value}`)} className="w-7 h-6"/></label>
        {command('No fill', () => p.onApplyFormat('Fill:'), 'Clear cell fill')}
      </>,
      Insert: <>
        {p.insertExtras}{command('Insert Table', p.onOpenInsertTable)}{command('Pivot', p.onOpenPivots)}{command('Charts', p.onOpenCharts)}
      </>,
      Data: <>
        {command('Databases', p.onOpenSchemaBrowser)}{command('SQL Query', p.onOpenQueryEditor)}
        {command('Refresh', p.onRefreshAll, 'Refresh All Data Sources')}{p.dataExtras}
      </>,
      Review: <>
        {command('Explain & Review', p.onOpenExplain)}{command('Open questions', p.onOpenQuestions)}
        <button type="button" disabled={p.busy} onClick={p.onOpenStagedReview}
          className={worksheetCommandClass + (p.stagedCount ? ' border-amber-300 bg-amber-50 text-amber-900' : '')}>
          Review Edits <span className="tabular-nums">{p.stagedCount}</span>
        </button>
        {command('Audit', p.onOpenAuditLog, 'Audit Log of Applied Writebacks')}
      </>,
      View: <>{p.viewExtras}</>,
      'Add-ons': <>
        <label className="flex shrink-0 items-center gap-2 rounded border border-slate-300 bg-white px-2 py-1">
          <input type="checkbox" checked={p.closeAddon} disabled={p.busy}
            onChange={e => p.onToggleCloseAddon(e.target.checked)} />Enable Monthly close add-on
        </label>
        {p.closeAddon && command('Monthly close', p.onOpenClose)}
        <span className="px-1 text-slate-600">Adds trial-balance and close reports. Switching it off does not remove any workbook data.</span>
      </>,
      Help: <>{command('About', p.onOpenAbout)}
        {command('Benchmarks', p.onOpenBenchmarks, 'Performance Acceptance Protocol')}</>,
  };
  return <div role="region" aria-label="Worksheet commands"
    className="flex shrink-0 items-center whitespace-nowrap border-b border-slate-200 bg-white px-1 text-xs select-none">
    <span className="hidden shrink-0 px-2 font-bold text-emerald-800 xl:inline">Tayriva</span>
    <div role="tablist" aria-label="Command groups"
      className="flex max-w-[54%] shrink-0 items-center overflow-x-auto border-r border-slate-200 px-1 py-1.5">
      {tabs.map((tab, index) => <button key={tab} ref={el => { tabRefs.current[index] = el; }} type="button" role="tab"
        id={`${id}-tab-${index}`} aria-controls={`${id}-panel-${index}`} aria-selected={tab === active}
        tabIndex={tab === active ? 0 : -1}
        onMouseDown={e => {
          // Changing groups must not blur a worksheet/formula draft.
          if (document.activeElement?.matches('input, textarea, [contenteditable="true"]')) e.preventDefault();
        }}
        onClick={() => activate(tab)}
        onKeyDown={event => navigate(event, index)}
        className={`shrink-0 rounded px-2 py-1 focus-visible:outline focus-visible:outline-2 focus-visible:outline-indigo-600 ${tab === active
          ? 'bg-emerald-100 font-semibold text-emerald-900'
          : 'font-medium text-slate-700 hover:bg-slate-100'}`}>{tab}</button>)}
    </div>
    <div ref={activePanel} className="min-w-0 flex-1 overflow-x-auto px-2 py-1.5">
      {tabs.map((tab,index) => <div key={tab} role="tabpanel" id={`${id}-panel-${index}`}
        aria-labelledby={`${id}-tab-${index}`} hidden={active !== tab}>
        <div className="flex w-max items-center gap-1">{commands[tab]}</div>
      </div>)}
    </div>
  </div>;
}
