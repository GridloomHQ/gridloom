import type { EvidenceReference, ReviewRecord } from '../types';

export function reviewStatus(review: ReviewRecord): string {
  if (review.invalidated) return 'Needs re-review';
  return review.decision === 'Question' ? 'Circled up' : 'Reviewed';
}

export function validateReview(reviewer: string, decision: 'Reviewed' | 'Question', note: string, evidence: EvidenceReference[]): string | null {
  if (!reviewer.trim()) return 'Enter a reviewer name or initials.';
  if (decision === 'Question' && !note.trim()) return 'Describe the question you are circling up.';
  if (evidence.some(e => !e.label.trim() || !e.reference.trim())) return 'Each evidence reference needs both a label and a reference.';
  return null;
}
