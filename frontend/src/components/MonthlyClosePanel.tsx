import React, { useEffect, useRef, useState } from 'react';
import { AdjustmentSchedule, ReportSetup } from './WorkingPaperControls';
import { api } from '../api';
import type { ClosePeriod, CloseStatus } from '../types';
import { liveMappingDraft, mappingDraftChanged, closeAmount } from '../utils/close';
import { exportPlCsv, exportCloseHtml } from '../utils/closeExports';
import { evidenceDownloadName } from '../utils/evidence';
import { displayValue } from '../utils/values';

interface Props { onClose: () => void; onChanged: () => void; onSelectCell: (sheet: string, row: number, col: number) => void }
const defaultSections = ['Revenue', 'Cost of sales', 'Operating expenses', 'Other income', 'Other expense', 'Balance sheet'];
const example = 'account,name,debit,credit\n1000,Cash,10000,0\n4000,Sales,0,15000\n5000,Cost of sales,5000,0';
export function MonthlyClosePanel({ onClose, onChanged, onSelectCell }: Props) {
  const [periods, setPeriods] = useState<ClosePeriod[]>([]);
  const [selected, setSelected] = useState('');
  const [status, setStatus] = useState<CloseStatus | null>(null);
  const [period, setPeriod] = useState(new Date().toISOString().slice(0, 7));
  const [csv, setCsv] = useState('');
  const [mappings, setMappings] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [setupDirty, setSetupDirty] = useState(false);
  const [adjustmentDirty, setAdjustmentDirty] = useState(false);
  const dialog = useRef<HTMLElement>(null);
  const running = useRef(false);
  const alive = useRef(true);
  const request = useRef(0);
  const field = 'border rounded px-2 py-1.5 bg-white text-slate-900';
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.current?.querySelector<HTMLElement>('button, input, select, textarea')?.focus();
    return () => { previous?.focus(); };
  }, []);
  useEffect(() => { alive.current = true; api.listClosePeriods().then(data => { if (alive.current) setPeriods(data); }).catch(e => { if (alive.current) setError(String(e)); }); return () => { alive.current = false; request.current++; }; }, []);
  useEffect(() => {
    const token = ++request.current;
    setStatus(null);
    if (!selected) return;
    api.getCloseStatus(selected).then(data => { if (alive.current && token === request.current) { setStatus(data); setMappings(liveMappingDraft(data.live_accounts)); } }).catch(e => { if (alive.current && token === request.current) setError(String(e)); });
  }, [selected]);
  const hasUnsavedMapping = status ? mappingDraftChanged(status.live_accounts, mappings) : false;
  const hasOtherDrafts = hasUnsavedMapping || setupDirty || adjustmentDirty;
  const sections = status?.period.report_lines?.length ? status.period.report_lines.filter(line => !status.period.report_lines?.some(child => child.parent_id === line.id))
    .map(line => ({id:line.id,label:line.label})) : defaultSections.map(label => ({id:label,label}));
  const custom = !!status?.period.report_lines?.length;
  const reportName = custom ? 'report' : 'P&L';
  const canLeave = () => !(csv.trim() || hasOtherDrafts) || confirm('Discard the unsaved working-paper drafts?');
  const requestClose = () => { if (!busy && canLeave()) onClose(); };
  async function run(action: () => Promise<void>) {
    if (running.current) return;
    running.current = true; setBusy(true); setError(''); setMessage('');
    try { await action(); } catch (e) { if (alive.current) setError(e instanceof Error ? e.message : String(e)); }
    finally { running.current = false; if (alive.current) setBusy(false); }
  }
  async function refresh(id: string) {
    const [list, data] = await Promise.all([api.listClosePeriods(), api.getCloseStatus(id)]);
    if (!alive.current) return;
    request.current++;
    setPeriods(list); setSelected(id); setStatus(data); setMappings(liveMappingDraft(data.live_accounts)); onChanged();
  }
  async function importCsv(rollforward: boolean) {
    if (hasOtherDrafts && !confirm('Discard unsaved working-paper changes before importing?')) return;
    await run(async () => {
      const result = rollforward ? await api.rollforwardClose({ source_period_id: selected, period, csv }) : await api.importTrialBalance({ period, csv, mappings: {} });
      await refresh(result.id); if (alive.current) setCsv(''); setMessage('Imported and generated P&L. Map every account, inspect reconciliation, then review each line. Save the workbook to retain this period.');
    });
  }
  const downloadPack = (format: 'json' | 'csv' | 'html') => void run(async () => {
    if (hasOtherDrafts) throw new Error('Save or discard the working-paper drafts before exporting.');
    const pack = await api.exportReviewPack(selected);
    const content = format === 'json' ? JSON.stringify(pack, null, 2) : format === 'csv' ? exportPlCsv(pack) : exportCloseHtml(pack);
    const mime = format === 'json' ? 'application/json' : format === 'csv' ? 'text/csv;charset=utf-8' : 'text/html;charset=utf-8';
    const url = URL.createObjectURL(new Blob([content], { type: mime }));
    const anchor = document.createElement('a'); anchor.href = url;
    anchor.download = evidenceDownloadName(`tayriva-${format === 'csv' ? 'pl' : format === 'html' ? 'close-summary' : 'review'}-${pack.status.period.period}-v${pack.status.period.version}.${format}`);
    anchor.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
    setMessage(format === 'json' ? 'Review pack downloaded as JSON, including captured inputs and attached evidence. This is not independent audit assurance.' : format === 'csv' ? 'P&L CSV downloaded with exact decimal text. Spreadsheet importers may round numeric amounts; use text import when exact preservation is required.' : 'Readable close summary downloaded. Keep the companion JSON pack for full captured inputs and support file bytes. This is not audit assurance.');
  });
  return <div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4" role="dialog" aria-modal="true" aria-label="Monthly close">
    <section ref={dialog} onKeyDown={e => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); requestClose(); }
      if (e.key === 'Tab') {
        const controls = Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), summary') ?? []).filter(el => el.getClientRects().length > 0);
        const first = controls[0], last = controls[controls.length - 1];
        if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
      }
    }} className="bg-slate-50 rounded-xl shadow-xl w-[1000px] max-w-full max-h-[92vh] overflow-y-auto text-sm">
      <header className="sticky top-0 bg-white border-b p-4 flex justify-between items-center z-10"><div><h2 className="font-bold text-lg">Monthly close</h2><p className="text-xs text-slate-500">Import → map → reconcile → explain and review → export</p></div><button aria-label="Close monthly close" disabled={busy} onClick={requestClose} className="p-2">✕</button></header>
      <div className="p-5 space-y-5">
        {error && <p role="alert" className="bg-rose-50 text-rose-800 p-3 break-words">{error}</p>}
        {message && <p role="status" className="bg-emerald-50 text-emerald-800 p-3">{message}</p>}
        <label className="block font-semibold">Existing period<select aria-label="Existing close period" disabled={busy} value={selected} onChange={e => { if (hasOtherDrafts && !confirm('Discard unsaved working-paper changes?')) return; setSelected(e.target.value); setError(''); }} className={`${field} ml-3`}><option value="">New import</option>{periods.map(p => <option key={p.id} value={p.id}>{p.period} · version {p.version} · {p.sheet_name}</option>)}</select></label>
        <details open={!selected} className="border rounded p-3 bg-white"><summary className="font-semibold cursor-pointer">Import trial balance / roll forward</summary><fieldset disabled={busy} className="space-y-3 mt-3">
          <p className="text-xs text-slate-600">CSV headers: account,name,debit,credit. Amounts use a decimal point, with no currency symbols or thousands separators. Each account must be unique. Rollforward requires a new month's trial balance and copies mappings, not approvals.</p>
          <label className="block">Period <input aria-label="Import period" type="month" value={period} onChange={e => setPeriod(e.target.value)} className={`${field} ml-2`} /></label>
          <label className="block">Trial balance CSV file<input type="file" accept=".csv,text/csv" className="block mt-1" onChange={e => { const file = e.target.files?.[0]; if (!file) return; if (file.size > 1024 * 1024) { setError('Trial balance CSV must be at most 1 MiB.'); return; } void run(async () => { const text = await file.text(); if (alive.current) setCsv(text); }); }} /></label>
          <label className="block">Or paste CSV<textarea aria-label="Trial balance CSV" value={csv} onChange={e => setCsv(e.target.value)} placeholder={example} rows={5} className={`${field} block w-full mt-1 font-mono text-xs`} /></label>
          <div className="flex flex-wrap gap-3"><button disabled={!csv.trim() || !period} onClick={() => void importCsv(false)} className="bg-indigo-700 text-white rounded px-3 py-2 disabled:opacity-50">{busy ? 'Working…' : 'Import and generate P&L'}</button><button disabled={!selected || !csv.trim() || !period} onClick={() => void importCsv(true)} className="border border-indigo-600 text-indigo-800 rounded px-3 py-2 disabled:opacity-50">Roll forward selected period</button></div>
        </fieldset></details>
        {status && <>
          <section className="border rounded p-4 bg-white space-y-2"><h3 className="font-semibold">Reconciliation · {status.period.period}</h3><p>{status.period.entity || 'Entity not specified'} · {status.period.currency || 'Currency not specified'} · {status.period.units || 'units'}</p><p>Debits {status.debits} · Credits {status.credits} · Difference <strong>{status.difference}</strong></p><p className={status.balanced ? 'text-emerald-700' : 'text-rose-700'}>{status.balanced ? 'Trial balance balances' : 'Trial balance does not balance'}</p><p>{status.ready_for_close ? 'All close checks and line reviews are current.' : 'Close is not ready. Resolve the checks and review queue below.'}</p>{status.warnings.map((w, i) => <p key={i} className="text-amber-900 text-xs">{w}</p>)}</section>
          <ReportSetup key={status.period.id} status={status} busy={busy || hasUnsavedMapping} run={run} refresh={() => refresh(selected)} onDirty={setSetupDirty} />
          <section className="border rounded p-4 bg-white space-y-3"><h3 className="font-semibold">Account mapping</h3><p className="text-xs text-slate-500">Imported net + posted adjustment net = reported net (debit-positive). Mappings assign each account to a report line and do not change imported facts. Unmapped: {status.unmapped_accounts.length}</p><div className="max-h-64 overflow-auto"><table className="w-full text-left text-xs"><thead><tr><th className="p-2">Account</th><th>Name</th><th>Imported net</th><th>Adjustments</th><th>Reported net</th><th>Report line</th></tr></thead><tbody>{status.live_accounts.map(a => <tr key={a.imported_account} className="border-t"><td className="p-2">{a.account || '(blank)'}{!a.identity_matches_import && <p className="text-rose-700 mt-1">Row {a.row + 1}: account changed from {a.imported_account}. Restore the imported identity in the worksheet before mapping.</p>}</td><td>{a.name}</td><td>{closeAmount(a.imported_balance ?? null)}</td><td>{closeAmount(a.adjustment_balance ?? null)}</td><td>{closeAmount(a.reported_balance ?? null)}</td><td className="py-1"><select aria-label={`Mapping ${a.imported_account}`} disabled={busy || setupDirty || !a.identity_matches_import} value={mappings[a.imported_account] ?? ''} onChange={e => setMappings(m => ({ ...m, [a.imported_account]: e.target.value }))} className={`${field} w-full`}><option value="">Unmapped</option>{mappings[a.imported_account] && !sections.some(section => section.id === mappings[a.imported_account]) && <option value={mappings[a.imported_account]}>{mappings[a.imported_account]} (invalid mapping)</option>}{sections.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}</select></td></tr>)}</tbody></table></div><button disabled={busy || setupDirty || status.live_accounts.some(a => !a.identity_matches_import)} onClick={() => void run(async () => { await api.mapCloseAccounts({ period_id: selected, mappings }); await refresh(selected); setMessage(`Mappings saved and ${reportName} recalculated.`); })} className="bg-indigo-700 text-white px-3 py-2 rounded disabled:opacity-50">Save account mappings</button></section>
          <AdjustmentSchedule key={status.period.id} status={status} busy={busy || hasUnsavedMapping || setupDirty} run={run} refresh={() => refresh(selected)} onDirty={setAdjustmentDirty} onSelectCell={(sheet,row,col) => { if (canLeave()) onSelectCell(sheet,row,col); }} />
          <section className="border rounded p-4 bg-white space-y-3"><div className="flex justify-between"><h3 className="font-semibold">Line review queue</h3><button disabled={busy} className="text-indigo-700 underline" onClick={() => void run(() => refresh(selected))}>Refresh checks</button></div><p className="text-xs text-slate-500">Open a line to explain its account inputs, attach support, ask a question or record a local review.</p>{status.queue.map(item => <button key={`${item.coord.row}:${item.coord.col}`} disabled={busy} onClick={() => { if (canLeave()) onSelectCell(status.period.sheet_name, item.coord.row, item.coord.col); }} className="w-full border rounded p-3 flex justify-between gap-3 hover:bg-indigo-50 text-left"><span>{item.label}</span><span className="font-mono">{displayValue(item.value)}</span><span className="text-indigo-700">{item.status} →</span></button>)}</section>
          <div className="flex flex-wrap gap-3" aria-label="Close exports">
            <button disabled={busy} onClick={() => downloadPack('json')} className="border border-indigo-600 text-indigo-800 rounded px-3 py-2 disabled:opacity-50">Export review pack</button>
            <button disabled={busy} onClick={() => downloadPack('csv')} className="border border-indigo-600 text-indigo-800 rounded px-3 py-2 disabled:opacity-50">{custom ? 'Download report CSV' : 'Download P&L CSV'}</button>
            <button disabled={busy} onClick={() => downloadPack('html')} className="border border-indigo-600 text-indigo-800 rounded px-3 py-2 disabled:opacity-50">Download readable close summary</button>
          </div>
          <p className="text-xs text-slate-500">HTML and CSV are readable summaries, not full evidence packages. Retain the JSON review pack alongside them. Exports use current worksheet data, not unsaved mapping drafts.</p>
        </>}
        <p className="text-xs text-slate-500">Local workbook reviews use self-reported identities. A balanced trial balance is not proof of completeness or accuracy. Save the workbook to persist imports, mappings and reviews.</p>
      </div>
    </section>
  </div>;
}
