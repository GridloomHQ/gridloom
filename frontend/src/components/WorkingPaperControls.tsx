import { useEffect, useState } from 'react';
import { api } from '../api';
import type { AdjustmentImpact, AdjustmentLine, CloseStatus, EvidenceAttachment, EvidenceUpload, ReportLine } from '../types';
import { evidenceDownloadName, validateAttachmentSizes } from '../utils/evidence';
import { displayValue } from '../utils/values';
import { coordToA1 } from '../utils/coords';

const field = 'border rounded px-2 py-1.5 bg-white text-slate-900 w-full';
const button = 'rounded px-3 py-2 bg-indigo-700 text-white disabled:opacity-50';
const defaultLine = (): AdjustmentLine => ({account:'', debit:'', credit:''});

function downloadEvidence(item: EvidenceAttachment) {
  const url = URL.createObjectURL(new Blob([new Uint8Array(item.bytes)], { type: 'application/octet-stream' }));
  const anchor = document.createElement('a');
  anchor.href = url; anchor.download = evidenceDownloadName(item.filename); anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

interface Shared {
  status: CloseStatus; busy: boolean; refresh: () => Promise<void>;
  run: (action: () => Promise<void>) => Promise<void>;
  onDirty: (dirty: boolean) => void;
}

export function ReportSetup({status, busy, refresh, run, onDirty}: Shared) {
  const {period} = status;
  const [entity, setEntity] = useState(period.entity ?? '');
  const [currency, setCurrency] = useState(period.currency ?? '');
  const [units, setUnits] = useState(period.units ?? 'units');
  const [lines, setLines] = useState<ReportLine[]>(period.report_lines ?? []);
  const [saved, setSaved] = useState(false);
  const dirty = entity !== (period.entity ?? '') || currency !== (period.currency ?? '') || units !== (period.units ?? 'units') || JSON.stringify(lines) !== JSON.stringify(period.report_lines ?? []);
  useEffect(() => { onDirty(dirty); }, [dirty, onDirty]);
  useEffect(() => () => onDirty(false), [onDirty]);
  const folders = new Set(lines.map(l => l.parent_id).filter(Boolean));
  const allowed = lines.length > 0 ? lines.filter(l => !folders.has(l.id)).map(l => l.id) :
    ['Revenue', 'Cost of sales', 'Operating expenses', 'Other income', 'Other expense', 'Balance sheet'];
  const lostMappings = status.live_accounts.filter(account => account.mapping && !allowed.includes(account.mapping));
  const update = (index: number, changes: Partial<ReportLine>) => {
    setSaved(false);
    setLines(rows => rows.map((line, i) => i === index ? {...line, ...changes} :
      changes.id !== undefined && line.parent_id === rows[index].id ? {...line, parent_id: changes.id || null} : line));
  };
  return <details className="border rounded p-4 bg-white">
    <summary className="cursor-pointer font-semibold">Entity, currency &amp; report hierarchy</summary>
    <form className="mt-3" onSubmit={event => {
      event.preventDefault();
      void run(async () => {
        const mappings = Object.fromEntries(status.live_accounts.map(a => [a.imported_account, allowed.includes(a.mapping) ? a.mapping : '']));
        await api.configureClose({period_id:period.id, entity, currency, units, report_lines:lines, mappings});
        setSaved(true); await refresh();
      });
    }}>
      <fieldset disabled={busy} className="space-y-3">
        <div className="grid sm:grid-cols-3 gap-3">
          <label>Entity<input required maxLength={200} className={field} value={entity} onChange={e => {setEntity(e.target.value);setSaved(false);}} /></label>
          <label>Currency code<input required pattern="[A-Z]{3}" placeholder="BRL" maxLength={3} className={field} value={currency} onChange={e => {setCurrency(e.target.value.toUpperCase());setSaved(false);}} /></label>
          <label>Units<input required maxLength={80} className={field} value={units} onChange={e => {setUnits(e.target.value);setSaved(false);}} /></label>
        </div>
        <p className="text-xs text-slate-600">A working paper covers one entity, period and currency. Units label the imported figures without scaling them. No FX conversion or consolidation is performed.</p>
        <p className="text-xs text-slate-600">Leave the hierarchy empty for the default P&amp;L. Map accounts to the lowest-level lines after saving. Parents roll up child accounts by debit/credit sign, expressed in the parent’s normal balance.</p>
        {lines.map((line,index) => <div key={index} className="grid gap-2 sm:grid-cols-[1fr_2fr_1.2fr_1fr_auto] border rounded p-2">
          <label>Line {index + 1} ID<input required pattern="[A-Za-z0-9_\-]+" maxLength={80} className={field} value={line.id} onChange={e => update(index,{id:e.target.value})} /></label>
          <label>Line {index + 1} label<input required maxLength={200} className={field} value={line.label} onChange={e => update(index,{label:e.target.value})} /></label>
          <label>Parent<select aria-label={`Parent for line ${index + 1}`} className={field} value={line.parent_id ?? ''} onChange={e => update(index,{parent_id:e.target.value || null})}>
            <option value="">Top level</option>{lines.map((candidate,i) => i !== index && candidate.id ? <option key={i} value={candidate.id}>{candidate.label || candidate.id}</option> : null)}
          </select></label>
          <label>Normal balance<select aria-label={`Normal balance for line ${index + 1}`} className={field} value={line.normal_balance} onChange={e => update(index,{normal_balance:e.target.value as ReportLine['normal_balance']})}><option>Debit</option><option>Credit</option></select></label>
          <button aria-label={`Remove report line ${index + 1}`} type="button" className="text-rose-700 self-end pb-2" onClick={() => {
            setSaved(false); setLines(rows => rows.filter((_,i) => i !== index).map(r => r.parent_id === line.id ? {...r,parent_id:null} : r));
          }}>Remove</button>
        </div>)}
        <div className="flex gap-3 flex-wrap">
          <button type="button" className="text-indigo-700 underline" disabled={lines.length >= 100} onClick={() => {setSaved(false); setLines(entries => [...entries, {id:'',label:'',parent_id:null,normal_balance:'Debit'}]);}}>Add report line</button>
          <button className={button} disabled={!dirty}>Save reporting setup</button>
        </div>
        {lostMappings.length > 0 && <p className="text-xs text-amber-900">Saving this hierarchy clears {lostMappings.length} incompatible account mapping{lostMappings.length === 1 ? '' : 's'}.</p>}
        {saved && <p role="status" className="text-emerald-800">Reporting setup saved.</p>}
      </fieldset>
    </form>
  </details>;
}

export function AdjustmentSchedule({status, busy, refresh, run, onDirty, onSelectCell}: Shared & {onSelectCell:(sheet:string,row:number,col:number)=>void}) {
  const [author, setAuthor] = useState('');
  const [rationale, setRationale] = useState('');
  const [lines, setLines] = useState<AdjustmentLine[]>([defaultLine(), defaultLine()]);
  const [evidence, setEvidence] = useState<EvidenceUpload[]>([]);
  const [posted, setPosted] = useState<AdjustmentImpact | null>(null);
  const dirty = Boolean(author || rationale || lines.length !== 2 || evidence.length || lines.some(l => l.account || l.debit || l.credit));
  useEffect(() => { onDirty(dirty); }, [dirty, onDirty]);
  useEffect(() => () => onDirty(false), [onDirty]);
  const update = (index:number, changes:Partial<AdjustmentLine>) => setLines(rows => rows.map((line,i) => i === index ? {...line,...changes} : line));
  const adjustments = status.period.adjustments ?? [];
  return <details className="border rounded p-4 bg-white">
    <summary className="cursor-pointer font-semibold">Adjustments &amp; supporting evidence ({adjustments.length})</summary>
    <p className="mt-3 text-xs text-slate-600">Post balanced working-paper entries, not ledger writeback. Imported balances are retained. To correct a posted entry, post a supported compensating entry.</p>
    {posted && <section aria-label="Adjustment impact" className="my-3 rounded border border-emerald-200 bg-emerald-50 p-3 space-y-2">
      <h4 className="font-semibold">Adjustment posted</h4>
      <p>{posted.affected_lines.length} report lines changed in amount or review status. {posted.invalidated_reviews.length} previously current sign-offs invalidated across the workbook.</p>
      {posted.affected_lines.map(line => <button type="button" className="block text-indigo-800 underline" key={line.coord.row} onClick={() => onSelectCell(status.period.sheet_name,line.coord.row,line.coord.col)}>
        {line.label}: {displayValue(line.before)} → {displayValue(line.after)}{line.before_status !== line.after_status ? ` · ${line.before_status} → ${line.after_status}` : ''}
      </button>)}
      {posted.invalidated_reviews.length > 0 && <details><summary>Invalidated sign-offs</summary><ul className="pl-4 list-disc">{posted.invalidated_reviews.map(review => <li key={review.id}>
        <button type="button" className="text-indigo-800 underline" onClick={() => onSelectCell(review.sheet_name,review.coord.row,review.coord.col)}>{review.sheet_name}!{coordToA1(review.coord.row,review.coord.col)} · {review.reviewer}</button>
      </li>)}</ul></details>}
    </section>}
    <form onSubmit={event => {
      event.preventDefault();
      void run(async () => {
        const impact = await api.postCloseAdjustment({period_id:status.period.id, author, rationale,
          lines:lines.map(line => ({...line,debit:line.debit.trim() || '0',credit:line.credit.trim() || '0'})), evidence});
        setPosted(impact); setAuthor(''); setRationale(''); setLines([defaultLine(), defaultLine()]); setEvidence([]);
        await refresh();
      });
    }}>
      <fieldset disabled={busy} className="space-y-3 mt-3">
        <label className="block">Adjustment author<input className={field} value={author} required maxLength={200} onChange={e => setAuthor(e.target.value)} /></label>
        <label className="block">Adjustment rationale<textarea className={field} value={rationale} required maxLength={4000} rows={2} onChange={e => setRationale(e.target.value)} /></label>
        {lines.map((line,index) => <div key={index} className="grid grid-cols-[2fr_1fr_1fr_auto] gap-2 border rounded p-2">
          <label>Account<select aria-label={`Adjustment account ${index + 1}`} required className={field} value={line.account} onChange={e => update(index,{account:e.target.value})}><option value="">Select account</option>{status.live_accounts.map(a => <option key={a.imported_account} value={a.imported_account}>{a.imported_account} · {a.name}</option>)}</select></label>
          <label>Debit<input aria-label={`Adjustment debit ${index + 1}`} inputMode="decimal" pattern="[0-9]+(\\.[0-9]+)?" className={field} value={line.debit} onChange={e => update(index,{debit:e.target.value})} /></label>
          <label>Credit<input aria-label={`Adjustment credit ${index + 1}`} inputMode="decimal" pattern="[0-9]+(\\.[0-9]+)?" className={field} value={line.credit} onChange={e => update(index,{credit:e.target.value})} /></label>
          <button type="button" aria-label={`Remove adjustment line ${index + 1}`} className="text-rose-700 self-end pb-2 disabled:opacity-40" disabled={lines.length <= 2} onClick={() => setLines(rows => rows.filter((_,i) => i !== index))}>Remove</button>
        </div>)}
        <button type="button" className="text-indigo-700 underline" disabled={lines.length >= 100} onClick={() => setLines(rows => [...rows, defaultLine()])}>Add adjustment line</button>
        <div>
          <label className="block">Adjustment support file(s)<input type="file" multiple className="block mt-1" onChange={e => {
            const files = Array.from(e.target.files ?? []); e.target.value = '';
            void run(async () => {
              const problem = validateAttachmentSizes(evidence.map(a => a.bytes.length), files.map(f => f.size));
              if (problem) throw new Error(problem);
              if (files.some(file => file.size === 0)) throw new Error('Evidence files must not be empty.');
              const uploads = await Promise.all(files.map(async f => ({filename:f.name,media_type:f.type || 'application/octet-stream',bytes:Array.from(new Uint8Array(await f.arrayBuffer()))})));
              setEvidence(previous => [...previous,...uploads]);
            });
          }} /></label>
          <p className="text-xs text-slate-600">At least one retained file. Maximum 2 MiB each, 8 MiB per entry. Files are retained, not verified.</p>
          {evidence.map((file,i) => <p className="break-all" key={i}>{file.filename} · {file.bytes.length} bytes <button type="button" className="text-rose-700" onClick={() => setEvidence(rows => rows.filter((_,j) => j !== i))}>Remove file</button></p>)}
        </div>
        <button className={button} disabled={!evidence.length}>Post adjustment</button>
      </fieldset>
    </form>
    {adjustments.length > 0 && <div className="space-y-2 mt-4"><h4 className="font-semibold">Posted journal entries</h4>
      {adjustments.map(journal => <details key={journal.id} className="rounded border p-2">
        <summary className="cursor-pointer">{journal.author} · {new Date(journal.created_at).toLocaleString()} · {journal.lines.length} lines</summary>
        <p className="whitespace-pre-wrap break-words">{journal.rationale}</p>
        <ul>{journal.lines.map((line,i) => <li key={i} className="font-mono text-xs">{line.account} · debit {line.debit} · credit {line.credit}</li>)}</ul>
        {journal.evidence.map(file => <div key={file.id} className="break-all"><button type="button" onClick={() => downloadEvidence(file)} className="text-indigo-700 underline">Download {file.filename}</button><p className="text-xs">SHA-256 {file.sha256}</p></div>)}
      </details>)}
    </div>}
  </details>;
}
