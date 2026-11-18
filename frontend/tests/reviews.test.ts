import test from 'node:test';
import assert from 'node:assert/strict';
import { reviewStatus, validateReview } from '../src/utils/reviews.ts';
import type { QuestionEvent, ReviewRecord } from '../src/types.ts';

test('review requires identity and circled-up questions require a note', () => {
  assert.match(validateReview(' ', 'Reviewed', '', [])!, /reviewer/);
  assert.match(validateReview('AB', 'Question', ' ', [])!, /question/);
  assert.equal(validateReview('AB', 'Reviewed', '', []), null);
  assert.equal(validateReview('AB', 'Question', 'Check cutoff', []), null);
});
test('evidence is explicit and complete, not just a label', () => {
  assert.match(validateReview('AB', 'Reviewed', '', [{label:'Invoice', reference:' '}])!, /both/);
  assert.equal(validateReview('AB', 'Reviewed', '', [{label:'Invoice', reference:'File 42, p.3'}]), null);
});
test('invalidated review never renders as currently reviewed', () => {
  const review = { decision:'Reviewed', invalidated:false } as ReviewRecord;
  assert.equal(reviewStatus(review), 'Reviewed');
  assert.equal(reviewStatus({...review, decision:'Question'}), 'Open question');
  assert.equal(reviewStatus({...review, invalidated:true}), 'Needs re-review');
  assert.equal(reviewStatus({...review, decision:'Question', invalidated:true}), 'Open question');
});

test('resolving and reopening are separate from input invalidation', () => {
  const resolved = { action:'Resolved' } as QuestionEvent;
  const reopened = { action:'Reopened' } as QuestionEvent;
  const question = { decision:'Question', invalidated:true, question_events:[resolved] } as ReviewRecord;
  assert.equal(reviewStatus(question), 'Resolved question');
  assert.equal(reviewStatus({...question, question_events:[resolved,reopened]}), 'Open question');
});
