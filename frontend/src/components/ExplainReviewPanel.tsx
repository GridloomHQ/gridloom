import React, { useEffect, useRef, useState } from 'react';
import { api } from '../api';
import { DependencyNavigation } from './DependencyNavigation';
import type { CellExplanation, EvidenceReference, ReviewRecord, ReviewDraft, EvidenceUpload, EvidenceAttachment, ReviewComparison, SourceDrilldown, QuestionAction, DependencyDirection, DependencyTarget } from '../types';
import { coordToA1 } from '../utils/coords';
import { displayValue } from '../utils/values';
import { validateAttachmentSizes, evidenceDownloadName } from '../utils/evidence';
import { questionIsOpen, reviewStatus, validateReview } from '../utils/reviews';

interface Props { drafts: Map<string, ReviewDraft>; sheetName: string; row: number; col: number; revision: string; onClose: () => void; onNavigate: (row: number, col: number, sheetName?: string) => void;
  onHighlight: (target: DependencyTarget | null, direction?: DependencyDirection) => void;
  path: {sheetName: string;row: number;col: number}[]; onBack: () => void }
function downloadEvidence(item: EvidenceAttachment) {
  // Download only: never render untrusted HTML/SVG attachments in this origin.
  const url = URL.createObjectURL(new Blob([new Uint8Array(item.bytes)], { type: 'application/octet-stream' }));
  const anchor = document.createElement('a'); anchor.href = url; anchor.download = evidenceDownloadName(item.filename); anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
function ReviewCard({ review }: { review: ReviewRecord }) {
  return <article className={`rounded border p-3 space-y-2 ${review.invalidated || review.decision === 'Question' ? 'border-amber-300 bg-amber-50' : 'border-emerald-200 bg-emerald-50'}`}>
    <div className="font-semibold">{reviewStatus(review)}</div>
    <p className="text-slate-600">{review.reviewer} · {new Date(review.created_at).toLocaleString()}</p>
    {review.invalidated && <p className="text-amber-900">{review.decision === 'Question' ? 'The calculation changed since this question was raised. Its captured inputs are retained.' : review.invalidation_reason || 'The reviewed calculation changed. Inspect its current inputs and review again.'}</p>}
    {review.question_events?.map(event => <div key={event.id} className="border-l-2 border-indigo-300 pl-2">
      <strong>{event.action}</strong> · {event.actor} · {new Date(event.created_at).toLocaleString()}
      <p className="whitespace-pre-wrap break-words">{event.note}</p>
    </div>)}
    {review.note && <p className="whitespace-pre-wrap break-words">{review.note}</p>}
    {review.attachments?.map(item => <div key={item.id} className="border-t pt-2 break-all"><button onClick={() => downloadEvidence(item)} className="text-indigo-700 underline">Download {item.filename} · v{item.version}</button><p className="text-[10px] text-slate-500">SHA-256 {item.sha256}</p><p className="text-slate-500">{item.bytes.length.toLocaleString()} bytes · Retained file, not verified contents</p></div>)}
    {review.evidence.map((item, i) => <div key={i} className="break-words"><strong>{item.label}</strong><p className="font-mono select-text">{item.reference}</p></div>)}
  </article>;
}
export function ExplainReviewPanel({ drafts, sheetName, row, col, revision, onClose, onNavigate, onHighlight, path, onBack }: Props) {
  const draftKey = JSON.stringify([sheetName, row, col]);
  const initialDraft = drafts.get(draftKey);
  const [explanation, setExplanation] = useState<CellExplanation | null>(null);
  const [history, setHistory] = useState<ReviewRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [reload, setReload] = useState(0);
  const [reviewer, setReviewer] = useState(initialDraft?.reviewer ?? '');
  const [decision, setDecision] = useState<'Reviewed' | 'Question'>(initialDraft?.decision ?? 'Reviewed');
  const [note, setNote] = useState(initialDraft?.note ?? '');
  const [evidence, setEvidence] = useState<EvidenceReference[]>(initialDraft?.evidence ?? []);
  const [attachments, setAttachments] = useState<EvidenceUpload[]>(initialDraft?.attachments ?? []);
  const [comparison, setComparison] = useState<ReviewComparison | null>(null);
  const [nodeLimit, setNodeLimit] = useState(100);
  const categoryNames: Record<string,string> = {
    formula:'Formula changed', input:'Input changed', mapping:'Account mapping changed',
    calculation:'Calculated value changed', source:'Cached source changed', source_state:'Source state changed',
    connection:'Connection changed', table:'Table changed', calculation_state:'Calculation state changed',
    reviewability:'Reviewability changed', evidence:'Review evidence changed', close_configuration:'Working paper changed',
    adjustment:'Posted adjustments changed', close_integrity:'Working-paper checks changed', working_paper_input:'Working-paper entry changed',
  };
  const [changeLimit, setChangeLimit] = useState(100);
  const [beforeId, setBeforeId] = useState('');
  const [afterId, setAfterId] = useState('');
  const [drilldown, setDrilldown] = useState<SourceDrilldown | null>(null);
  const [transition, setTransition] = useState<{ question: ReviewRecord; action: QuestionAction } | null>(null);
  const [transitionActor, setTransitionActor] = useState('');
  const [transitionNote, setTransitionNote] = useState('');
  const signoffBlocked = !explanation?.can_review || history.some(questionIsOpen);
  const effectiveDecision = signoffBlocked ? 'Question' : decision;
  const retainedAttachments = history.flatMap(r => r.attachments ?? []);
  const supersededIds = new Set(retainedAttachments.map(a => a.previous_version_id).filter(Boolean));
  const latestAttachments = retainedAttachments.filter(a => !supersededIds.has(a.id));
  const saving = useRef(false);
  const mounted = useRef(true);
  const inputClass = 'w-full border border-slate-300 rounded px-2 py-1.5 bg-white text-slate-900 focus:outline-indigo-500';
  useEffect(() => { drafts.set(draftKey, { reviewer, decision, note, evidence, attachments }); }, [drafts, draftKey, reviewer, decision, note, evidence, attachments]);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => {
    let active = true;
    setLoading(true); setExplanation(null); setComparison(null); setDrilldown(null); setNodeLimit(100); setChangeLimit(100); setError('');
    Promise.all([api.explainCell(sheetName, row, col), api.getReviews()]).then(([data, records]) => {
      if (!active) return;
      setExplanation(data);
      setHistory(records.filter(r => r.sheet_name === sheetName && r.coord.row === row && r.coord.col === col));
    }).catch(e => { if (active) setError(e instanceof Error ? e.message : 'Unable to load explanation.'); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [sheetName, row, col, revision, reload]);
  const save = async (event: React.FormEvent) => {
    event.preventDefault();
    if (saving.current || !explanation || loading) return;
    const problem = validateReview(reviewer, effectiveDecision, note, evidence);
    if (problem) { setError(problem); return; }
    saving.current = true; setBusy(true); setError(''); setMessage('');
    try {
      await api.recordAttachedReview({ review: { sheet_name: sheetName, row, col, expected_snapshot: explanation.snapshot, decision: effectiveDecision,
        reviewer: reviewer.trim(), note: note.trim(), evidence: evidence.map(e => ({ label: e.label.trim(), reference: e.reference.trim() })) }, attachments });
      if (!mounted.current) return;
      setNote(''); setEvidence([]); setAttachments([]);
      setMessage(effectiveDecision === 'Question' ? 'Question opened in this workbook.' : 'Review recorded in this workbook. Save the workbook to keep it on disk.');
      setReload(n => n + 1);
    } catch (e) {
      if (mounted.current) setError(`${e instanceof Error ? e.message : 'Could not record review.'} Refresh the explanation before retrying. Your draft is preserved.`);
    } finally { saving.current = false; if (mounted.current) setBusy(false); }
  };
  const inspect = async (action: () => Promise<void>) => {
    if (saving.current) return;
    saving.current = true; setBusy(true); setError('');
    try { await action(); } catch (e) { if (mounted.current) setError(e instanceof Error ? e.message : String(e)); }
    finally { saving.current = false; if (mounted.current) setBusy(false); }
  };
  const transitionQuestion = (event: React.FormEvent) => {
    event.preventDefault();
    if (!explanation || loading || !transition || !transitionActor.trim() || !transitionNote.trim()) return;
    const { question, action } = transition;
    void inspect(async () => {
      await api.transitionQuestion(question.id, {
        action, actor: transitionActor.trim(), note: transitionNote.trim(), expected_snapshot: explanation.snapshot,
        expected_last_event_id: question.question_events?.[question.question_events.length - 1]?.id ?? null,
      });
      if (!mounted.current) return;
      setTransition(null); setTransitionNote(''); setMessage(action === 'Resolved' ? 'Question resolved. This does not record a review.' : 'Question reopened.'); setReload(n => n + 1);
    });
  };
  return <aside aria-label="Explain and review selected cell" className="w-[400px] max-w-[85vw] shrink-0 border-l border-slate-300 bg-slate-50 overflow-y-auto text-xs select-text">
    <header className="sticky top-0 z-10 bg-white border-b p-3 flex items-center justify-between gap-2">
      <div><h2 className="font-bold text-sm">Explain &amp; Review</h2><p className="text-slate-500 mt-1">{sheetName} · {coordToA1(row, col)}</p></div>
      <div className="flex items-center gap-2"><button disabled={loading || busy} onClick={() => setReload(n => n + 1)} className="text-indigo-700 underline disabled:opacity-50">Refresh</button><button aria-label="Close explanation" onClick={onClose} className="p-2 rounded hover:bg-slate-100">✕</button></div>
    </header>
    <div className="p-4 space-y-5">
      {path.length > 0 && <nav aria-label="Dependency trail" className="flex flex-wrap gap-2 items-center text-slate-600">
        <button disabled={busy} onClick={onBack} className="text-indigo-700 underline">Back one cell</button>
        <span className="break-all">{path.map(step => step.sheetName + '!' + coordToA1(step.row, step.col)).join(' → ')} → {sheetName}!{coordToA1(row, col)}</span>
      </nav>}
      {error && <p role="alert" className="rounded bg-rose-50 text-rose-800 p-3 break-words">{error}</p>}
      {message && <p role="status" className="rounded bg-emerald-50 text-emerald-800 p-3">{message}</p>}
      {loading && <p role="status">Loading current calculation…</p>}
      {explanation && <>
        <section className="rounded bg-white border p-3 space-y-2"><h3 className="text-slate-500 font-semibold">Current calculated value</h3><p className="text-xl font-mono break-words">{displayValue(explanation.value) || '(blank)'}</p>{explanation.formula && <code className="block break-all text-indigo-800">{explanation.formula}</code>}</section>
        {signoffBlocked && <p className="text-amber-900">You can record a question, but cannot mark this calculation reviewed yet.</p>}
        <DependencyNavigation sheetName={sheetName} row={row} col={col} revision={revision} onNavigate={onNavigate} onHighlight={onHighlight} onDrilldown={name => void inspect(async () => {
          const data = await api.getSourceDrilldown(name, 0, 50); if (mounted.current) setDrilldown(data);
        })} />
        <details className="space-y-2"><summary className="font-bold cursor-pointer">Captured dependency details</summary><p className="text-slate-500">References include all formula branches. SQL stops at the query/result boundary; this is not contributing-row provenance.</p>
          {explanation.nodes.slice(0, nodeLimit).map(node => <article key={node.id} className="border rounded bg-white p-3 space-y-1 break-words">
            <div className="flex items-center justify-between gap-2"><span className="font-semibold">{node.label}</span><span className="uppercase text-[10px] text-slate-500">{node.kind}</span></div>
            {node.value !== null && <p className="font-mono">{displayValue(node.value) || '(blank)'}</p>}
            {node.formula && <code className="block break-all text-indigo-800">{node.formula}</code>}
            {node.detail && <p className="whitespace-pre-wrap text-slate-600">{node.detail}</p>}
            {node.depends_on.length > 0 && <p className="text-slate-500">Uses: {node.depends_on.slice(0, 20).map(id => explanation.nodes.find(n => n.id === id)?.label ?? id).join(', ') + (node.depends_on.length > 20 ? ` … and ${node.depends_on.length - 20} more captured dependencies` : '')}</p>}
            {node.coord && (node.coord.row !== row || node.coord.col !== col || (node.sheet_name && node.sheet_name !== sheetName)) && <button className="text-indigo-700 underline" disabled={busy} onClick={() => {
              onNavigate(node.coord!.row, node.coord!.col, node.sheet_name ?? sheetName);
            }}>Inspect {coordToA1(node.coord.row, node.coord.col)}</button>}
            {node.source_name && <button disabled={busy} className="text-indigo-700 underline block" onClick={() => void inspect(async () => { const data = await api.getSourceDrilldown(node.source_name!, 0, 50); if (mounted.current) setDrilldown(data); })}>Check source-row drilldown</button>}
          </article>)}
          {nodeLimit < explanation.nodes.length && <button className="text-indigo-700 underline" onClick={() => setNodeLimit(n => n + 100)}>Show next 100 dependencies ({nodeLimit} of {explanation.nodes.length} shown)</button>}
        </details>
        {drilldown && <section className="border rounded p-3 bg-white space-y-2"><h3 className="font-bold">{drilldown.source_name} · cached source rows</h3><p className="text-amber-900">{drilldown.warning}</p><code className="block break-all">{drilldown.sql}</code><div className="overflow-auto max-h-72"><table className="text-xs"><thead><tr><th>Key</th>{drilldown.columns.map((c, i) => <th className="p-2" key={i}>{c}</th>)}</tr></thead><tbody>{drilldown.rows.map(r => <tr key={r.cached_row_index} className="border-t"><td className="p-2">{Object.entries(r.primary_key).map(([k, v]) => `${k}=${displayValue(v)}`).join(', ')}</td>{r.values.map((v, i) => <td key={i} className="p-2 whitespace-nowrap">{displayValue(v)}</td>)}</tr>)}</tbody></table></div><div className="flex justify-between"><button disabled={busy || drilldown.offset === 0} onClick={() => void inspect(async () => { const data = await api.getSourceDrilldown(drilldown.source_name, Math.max(0, drilldown.offset - 50), 50); if (mounted.current) setDrilldown(data); })}>Previous</button><span>{drilldown.offset + 1}–{drilldown.offset + drilldown.rows.length} of {drilldown.total_rows}</span><button disabled={busy || drilldown.offset + drilldown.rows.length >= drilldown.total_rows} onClick={() => void inspect(async () => { const data = await api.getSourceDrilldown(drilldown.source_name, drilldown.offset + 50, 50); if (mounted.current) setDrilldown(data); })}>Next</button></div></section>}
        {history.length > 0 && <section className="space-y-2 border-t pt-3"><h3 className="font-bold">Compare captured inputs</h3><label className="block">Before<select className={inputClass} value={beforeId} disabled={busy} onChange={e => setBeforeId(e.target.value)}><option value="">Latest review</option>{history.map(r => <option key={r.id} value={r.id}>{new Date(r.created_at).toLocaleString()} · {r.reviewer}</option>)}</select></label><label className="block">After<select className={inputClass} value={afterId} disabled={busy} onChange={e => setAfterId(e.target.value)}><option value="">Current calculation</option>{history.map(r => <option key={r.id} value={r.id}>{new Date(r.created_at).toLocaleString()} · {r.reviewer}</option>)}</select></label><button disabled={busy} className="text-indigo-700 underline" onClick={() => void inspect(async () => { const data = await api.getReviewComparison(beforeId || history[history.length - 1].id, afterId || undefined); if (mounted.current) { setComparison(data); setChangeLimit(100); } })}>Compare exact changes</button>{comparison && <div className="space-y-2">
          <p>{comparison.changes.length} captured input changes</p>
          {!!comparison.summaries?.length && <ul className="list-disc pl-4 space-y-1" aria-label="Accounting change summary">
            {comparison.summaries.slice(0, changeLimit).map((change, i) => <li key={i}><strong>{categoryNames[change.category] ?? change.category}:</strong> {change.label}</li>)}
          </ul>}
          {!!comparison.affected_cells?.length && <div><h4 className="font-semibold">Changed cells in this captured trace</h4>{comparison.affected_cells.slice(0, changeLimit).map(cell => <button key={JSON.stringify([cell.sheet_name, cell.coord])} className="block text-indigo-700 underline" onClick={() => onNavigate(cell.coord.row, cell.coord.col, cell.sheet_name)}>{cell.label}</button>)}</div>}
          {!!comparison.changes.length && <details className="space-y-2"><summary className="cursor-pointer font-semibold">Exact before / after fields</summary>{comparison.changes.slice(0, changeLimit).map((c, i) => <details key={i} className="border rounded p-2 bg-white"><summary className="break-all cursor-pointer">{c.path}</summary><h4 className="font-bold mt-2">Before</h4><pre className="whitespace-pre-wrap break-all max-h-64 overflow-auto">{JSON.stringify(c.before, null, 2) ?? '(absent)'}</pre><h4 className="font-bold mt-2">After</h4><pre className="whitespace-pre-wrap break-all max-h-64 overflow-auto">{JSON.stringify(c.after, null, 2) ?? '(absent)'}</pre></details>)}{changeLimit < comparison.changes.length && <button className="text-indigo-700 underline" onClick={() => setChangeLimit(n => n + 100)}>Show next 100 changes ({changeLimit} of {comparison.changes.length} shown)</button>}</details>}</div>}</section>}
        {history.some(record => record.decision === 'Question') && <section className="space-y-2 border-t pt-4" aria-label="Questions on this cell">
          <h3 className="font-bold">Questions on this cell</h3>
          {history.filter(record => record.decision === 'Question').map(question => <div key={question.id} className="space-y-1">
            <ReviewCard review={question} />
            <button disabled={busy} className="text-indigo-700 underline" onClick={() => {
              setTransition({question, action: questionIsOpen(question) ? 'Resolved' : 'Reopened'});
              setTransitionActor(reviewer); setTransitionNote('');
            }}>{questionIsOpen(question) ? 'Resolve question' : 'Reopen question'}</button>
          </div>)}
          {transition && <form onSubmit={transitionQuestion} className="rounded border p-3 bg-white space-y-2">
            <h4 className="font-semibold">{transition.action === 'Resolved' ? 'Resolve this question' : 'Reopen this question'}</h4>
            <p className="whitespace-pre-wrap break-words">{transition.question.note}</p>
            <fieldset disabled={busy} className="space-y-2">
              <label className="block">Name or initials<input required maxLength={120} className={inputClass} value={transitionActor} onChange={e => setTransitionActor(e.target.value)} /></label>
              <label className="block">{transition.action === 'Resolved' ? 'Resolution note' : 'Reason for reopening'}<textarea required maxLength={4000} rows={3} className={inputClass} value={transitionNote} onChange={e => setTransitionNote(e.target.value)} /></label>
              <div className="flex gap-3"><button type="submit" className="rounded bg-indigo-700 px-3 py-1.5 text-white">{transition.action === 'Resolved' ? 'Save resolution' : 'Save reopening'}</button><button type="button" onClick={() => setTransition(null)}>Cancel</button></div>
            </fieldset>
          </form>}
        </section>}
        {explanation.review && explanation.review.decision === 'Reviewed' && <section className="space-y-2"><h3 className="font-bold">Latest review</h3><ReviewCard review={explanation.review} /></section>}
        <form onSubmit={save} className="space-y-3 border-t pt-4"><h3 className="font-bold">Review this calculation</h3><p className="text-slate-500">Review records stay inside the .gridloom file. Names are self-reported, not authenticated sign-offs.</p>
          <fieldset disabled={busy || loading} className="space-y-3 disabled:opacity-60">
            <label className="block">Reviewer name or initials<input required maxLength={120} value={reviewer} onChange={e => setReviewer(e.target.value)} className={`${inputClass} mt-1`} /></label>
            <label className="block">Decision<select value={effectiveDecision} onChange={e => setDecision(e.target.value as 'Reviewed' | 'Question')} className={`${inputClass} mt-1`}><option disabled={signoffBlocked} value="Reviewed">Mark reviewed</option><option value="Question">Circle up a question</option></select></label>
            <label className="block">{effectiveDecision === 'Question' ? 'Question (required)' : 'Review note'}<textarea required={effectiveDecision === 'Question'} maxLength={4000} value={note} onChange={e => setNote(e.target.value)} rows={3} className={`${inputClass} mt-1`} /></label>
            <div className="space-y-2"><h4 className="font-semibold">Evidence references</h4><p className="text-slate-500">Add a document name, URL, page or workpaper reference. These are notes only: files are not uploaded, fetched or verified.</p>
              {evidence.map((item, index) => <div key={index} className="border rounded p-2 space-y-2 bg-white">
                <label className="block">Label<input required maxLength={200} value={item.label} onChange={e => setEvidence(items => items.map((x, i) => i === index ? { ...x, label: e.target.value } : x))} className={inputClass} /></label>
                <label className="block">Reference<input required maxLength={2000} value={item.reference} onChange={e => setEvidence(items => items.map((x, i) => i === index ? { ...x, reference: e.target.value } : x))} className={inputClass} /></label>
                <button type="button" aria-label={`Remove evidence ${index + 1}`} onClick={() => setEvidence(items => items.filter((_, i) => i !== index))} className="text-rose-700">Remove</button>
              </div>)}
              <button disabled={evidence.length >= 20} type="button" onClick={() => setEvidence(items => [...items, { label: '', reference: '' }])} className="text-indigo-700">+ Add reference</button>
            </div>
            <div className="space-y-2"><h4 className="font-semibold">Attach versioned support</h4><p className="text-slate-500">Files are retained in this review (2 MiB each, 8 MiB total). Contents are not verified. Download files only from sources you trust.</p><input aria-label="Attach support files" type="file" multiple onChange={e => { const files = Array.from(e.target.files ?? []); e.target.value = ''; void inspect(async () => { const problem = validateAttachmentSizes(attachments.map(a => a.bytes.length), files.map(f => f.size)); if (problem) throw new Error(problem); const uploads = await Promise.all(files.map(async f => ({ filename: f.name, media_type: f.type || 'application/octet-stream', bytes: Array.from(new Uint8Array(await f.arrayBuffer())) }))); if (mounted.current) setAttachments(a => [...a, ...uploads]); }); }} />{attachments.map((file, i) => <div key={i} className="border rounded p-2 bg-white space-y-1"><p className="break-all">{file.filename} · {file.bytes.length.toLocaleString()} bytes</p><label className="block">Version of<select className={inputClass} value={file.replaces ?? ''} onChange={e => setAttachments(a => a.map((f, n) => n === i ? { ...f, replaces: e.target.value || undefined } : f))}><option value="">New evidence item</option>{latestAttachments.map(a => <option key={a.id} value={a.id}>{a.filename} · v{a.version}</option>)}</select></label><button type="button" className="text-rose-700" onClick={() => setAttachments(a => a.filter((_, n) => n !== i))}>Remove attachment</button></div>)}</div>
            <button type="submit" className="w-full bg-indigo-700 text-white rounded px-3 py-2 font-semibold hover:bg-indigo-800">{busy ? 'Recording…' : effectiveDecision === 'Question' ? 'Circle up this cell' : 'Record review'}</button>
          </fieldset>
          <p className="text-slate-500">Changes to the calculation or its inputs can invalidate a review. Drafts are kept per cell for this session only. Record a review and save the workbook to retain it.</p>
        </form>
        {history.filter(r => r.decision === 'Reviewed').length > 1 && <details className="border-t pt-3"><summary className="cursor-pointer font-semibold">Review history ({history.length})</summary><div className="mt-3 space-y-2">{history.filter(r => r.decision === 'Reviewed').map(review => <ReviewCard key={review.id} review={review} />)}</div></details>}
      </>}
    </div>
  </aside>;
}
