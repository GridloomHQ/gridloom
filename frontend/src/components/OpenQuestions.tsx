import { useEffect, useState } from 'react';
import { api } from '../api';
import type { ReviewRecord } from '../types';
import { coordToA1 } from '../utils/coords';

interface Props {
  onClose: () => void;
  onNavigate: (question: ReviewRecord) => void;
}

export function OpenQuestions({ onClose, onNavigate }: Props) {
  const [questions, setQuestions] = useState<ReviewRecord[] | null>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    let active = true;
    api.getOpenQuestions().then(result => { if (active) setQuestions(result); })
      .catch(e => { if (active) setError(e instanceof Error ? e.message : String(e)); });
    return () => { active = false; };
  }, []);
  return <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4">
    <section role="dialog" aria-modal="true" aria-labelledby="questions-title" onKeyDown={e => { if (e.key === 'Escape') onClose(); }} className="flex max-h-[85vh] w-full max-w-2xl flex-col rounded-lg bg-slate-50 shadow-xl">
      <header className="flex items-start justify-between border-b bg-white p-4 rounded-t-lg">
        <div><h2 id="questions-title" className="font-bold">Open questions</h2><p className="text-xs text-slate-600 mt-1">Across all worksheets in this workbook. Resolving a question does not sign off a calculation.</p></div>
        <button autoFocus onClick={onClose} className="rounded p-2 hover:bg-slate-100" aria-label="Close open questions">✕</button>
      </header>
      <div className="overflow-auto p-4 space-y-2">
        {error ? <p role="alert" className="text-rose-700">{error}</p> : questions === null ? <p role="status">Loading open questions…</p>
          : questions.length === 0 ? <p role="status">No open questions.</p>
            : questions.map(q => <button className="block w-full rounded border border-amber-200 bg-white p-3 text-left hover:bg-amber-50" key={q.id} onClick={() => onNavigate(q)}>
              <span className="block font-semibold text-indigo-800">{q.sheet_name} · {coordToA1(q.coord.row, q.coord.col)} →</span>
              <span className="block mt-1 whitespace-pre-wrap break-words">{q.note}</span>
              <span className="block mt-1 text-xs text-slate-500">{q.reviewer} · {new Date(q.created_at).toLocaleString()}{q.invalidated ? ' · Calculation changed since raised' : ''}</span>
            </button>)}
      </div>
    </section>
  </div>;
}
