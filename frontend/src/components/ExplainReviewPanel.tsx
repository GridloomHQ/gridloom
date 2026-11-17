import React, { useEffect, useRef, useState } from 'react';
import { api } from '../api';
import type { CellExplanation, EvidenceReference, ReviewRecord, ReviewDraft } from '../types';
import { coordToA1 } from '../utils/coords';
import { displayValue } from '../utils/values';
import { reviewStatus, validateReview } from '../utils/reviews';

interface Props { drafts: Map<string, ReviewDraft>; sheetName: string; row: number; col: number; revision: string; onClose: () => void; onNavigate: (row: number, col: number) => void }
function ReviewCard({ review }: { review: ReviewRecord }) {
  return <article className={`rounded border p-3 space-y-2 ${review.invalidated || review.decision === 'Question' ? 'border-amber-300 bg-amber-50' : 'border-emerald-200 bg-emerald-50'}`}>
    <div className="font-semibold">{reviewStatus(review)}</div>
    <p className="text-slate-600">{review.reviewer} · {new Date(review.created_at).toLocaleString()}</p>
    {review.invalidated && <p className="text-amber-900">{review.invalidation_reason || 'The reviewed calculation changed. Inspect its current inputs and review again.'}</p>}
    {review.note && <p className="whitespace-pre-wrap break-words">{review.note}</p>}
    {review.evidence.map((item, i) => <div key={i} className="break-words"><strong>{item.label}</strong><p className="font-mono select-text">{item.reference}</p></div>)}
  </article>;
}
export function ExplainReviewPanel({ drafts, sheetName, row, col, revision, onClose, onNavigate }: Props) {
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
  const saving = useRef(false);
  const mounted = useRef(true);
  const inputClass = 'w-full border border-slate-300 rounded px-2 py-1.5 bg-white text-slate-900 focus:outline-indigo-500';
  useEffect(() => { drafts.set(draftKey, { reviewer, decision, note, evidence }); }, [drafts, draftKey, reviewer, decision, note, evidence]);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => {
    let active = true;
    setLoading(true); setExplanation(null); setError('');
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
    if (saving.current || !explanation || loading || !explanation.can_review) return;
    const problem = validateReview(reviewer, decision, note, evidence);
    if (problem) { setError(problem); return; }
    saving.current = true; setBusy(true); setError(''); setMessage('');
    try {
      await api.saveReview({ sheet_name: sheetName, row, col, expected_snapshot: explanation.snapshot, decision,
        reviewer: reviewer.trim(), note: note.trim(), evidence: evidence.map(e => ({ label: e.label.trim(), reference: e.reference.trim() })) });
      if (!mounted.current) return;
      setNote(''); setEvidence([]);
      setMessage('Review recorded in this workbook. Save the workbook to keep it on disk.');
      setReload(n => n + 1);
    } catch (e) {
      if (mounted.current) setError(`${e instanceof Error ? e.message : 'Could not record review.'} Refresh the explanation before retrying. Your draft is preserved.`);
    } finally { saving.current = false; if (mounted.current) setBusy(false); }
  };
  return <aside aria-label="Explain and review selected cell" className="w-[400px] max-w-[85vw] shrink-0 border-l border-slate-300 bg-slate-50 overflow-y-auto text-xs select-text">
    <header className="sticky top-0 z-10 bg-white border-b p-3 flex items-center justify-between">
      <div><h2 className="font-bold text-sm">Explain &amp; Review</h2><p className="text-slate-500 mt-1">{sheetName} · {coordToA1(row, col)}</p></div>
      <button aria-label="Close explanation" onClick={onClose} className="p-2 rounded hover:bg-slate-100">✕</button>
    </header>
    <div className="p-4 space-y-5">
      <div className="flex justify-between gap-2"><span className="text-slate-500">Calculation dependencies, not audit assurance</span><button disabled={loading || busy} onClick={() => setReload(n => n + 1)} className="text-indigo-700 underline disabled:opacity-50">Refresh</button></div>
      {error && <p role="alert" className="rounded bg-rose-50 text-rose-800 p-3 break-words">{error}</p>}
      {message && <p role="status" className="rounded bg-emerald-50 text-emerald-800 p-3">{message}</p>}
      {loading && <p role="status">Loading current calculation…</p>}
      {explanation && <>
        <section className="rounded bg-white border p-3 space-y-2"><h3 className="text-slate-500 font-semibold">Current calculated value</h3><p className="text-xl font-mono break-words">{displayValue(explanation.value) || '(blank)'}</p>{explanation.formula && <code className="block break-all text-indigo-800">{explanation.formula}</code>}</section>
        {explanation.warnings.length > 0 && <ul className="list-disc pl-5 space-y-1 text-amber-900">{explanation.warnings.map((w, i) => <li key={i}>{w}</li>)}</ul>}
        <section className="space-y-2"><h3 className="font-bold">What this number depends on</h3><p className="text-slate-500">References include all formula branches. SQL stops at the query/result boundary; this is not contributing-row provenance.</p>
          {explanation.nodes.map(node => <article key={node.id} className="border rounded bg-white p-3 space-y-1 break-words">
            <div className="flex items-center justify-between gap-2"><span className="font-semibold">{node.label}</span><span className="uppercase text-[10px] text-slate-500">{node.kind}</span></div>
            {node.value !== null && <p className="font-mono">{displayValue(node.value) || '(blank)'}</p>}
            {node.formula && <code className="block break-all text-indigo-800">{node.formula}</code>}
            {node.detail && <p className="whitespace-pre-wrap text-slate-600">{node.detail}</p>}
            {node.depends_on.length > 0 && <p className="text-slate-500">Uses: {node.depends_on.map(id => explanation.nodes.find(n => n.id === id)?.label ?? id).join(', ')}</p>}
            {node.coord && (node.coord.row !== row || node.coord.col !== col) && <button className="text-indigo-700 underline" disabled={busy} onClick={() => {
              onNavigate(node.coord!.row, node.coord!.col);
            }}>Inspect {coordToA1(node.coord.row, node.coord.col)}</button>}
          </article>)}
        </section>
        {explanation.review && <section className="space-y-2"><h3 className="font-bold">Latest review</h3><ReviewCard review={explanation.review} /></section>}
        <form onSubmit={save} className="space-y-3 border-t pt-4"><h3 className="font-bold">Review this calculation</h3><p className="text-slate-500">Review records stay inside the .gridloom file. Names are self-reported, not authenticated sign-offs.</p>
          <fieldset disabled={busy || loading || !explanation.can_review} className="space-y-3 disabled:opacity-60">
            <label className="block">Reviewer name or initials<input required maxLength={120} value={reviewer} onChange={e => setReviewer(e.target.value)} className={`${inputClass} mt-1`} /></label>
            <label className="block">Decision<select value={decision} onChange={e => setDecision(e.target.value as 'Reviewed' | 'Question')} className={`${inputClass} mt-1`}><option value="Reviewed">Mark reviewed</option><option value="Question">Circle up a question</option></select></label>
            <label className="block">{decision === 'Question' ? 'Question (required)' : 'Review note'}<textarea required={decision === 'Question'} maxLength={4000} value={note} onChange={e => setNote(e.target.value)} rows={3} className={`${inputClass} mt-1`} /></label>
            <div className="space-y-2"><h4 className="font-semibold">Evidence references</h4><p className="text-slate-500">Add a document name, URL, page or workpaper reference. These are notes only: files are not uploaded, fetched or verified.</p>
              {evidence.map((item, index) => <div key={index} className="border rounded p-2 space-y-2 bg-white">
                <label className="block">Label<input required maxLength={200} value={item.label} onChange={e => setEvidence(items => items.map((x, i) => i === index ? { ...x, label: e.target.value } : x))} className={inputClass} /></label>
                <label className="block">Reference<input required maxLength={2000} value={item.reference} onChange={e => setEvidence(items => items.map((x, i) => i === index ? { ...x, reference: e.target.value } : x))} className={inputClass} /></label>
                <button type="button" aria-label={`Remove evidence ${index + 1}`} onClick={() => setEvidence(items => items.filter((_, i) => i !== index))} className="text-rose-700">Remove</button>
              </div>)}
              <button disabled={evidence.length >= 20} type="button" onClick={() => setEvidence(items => [...items, { label: '', reference: '' }])} className="text-indigo-700">+ Add reference</button>
            </div>
            <button type="submit" className="w-full bg-indigo-700 text-white rounded px-3 py-2 font-semibold hover:bg-indigo-800">{busy ? 'Recording…' : decision === 'Question' ? 'Circle up this cell' : 'Record review'}</button>
          </fieldset>
          {!explanation.can_review && <p className="text-amber-900">This calculation cannot be marked reviewed yet. Resolve the explanation warnings and refresh.</p>}
          <p className="text-slate-500">Changes to the calculation or its inputs can invalidate a review. Drafts are kept per cell for this session only. Record a review and save the workbook to retain it.</p>
        </form>
        {history.length > 1 && <details className="border-t pt-3"><summary className="cursor-pointer font-semibold">Review history ({history.length})</summary><div className="mt-3 space-y-2">{history.map(review => <ReviewCard key={review.id} review={review} />)}</div></details>}
      </>}
    </div>
  </aside>;
}
