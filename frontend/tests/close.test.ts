import test from 'node:test';
import assert from 'node:assert/strict';
import { liveMappingDraft, mappingDraftChanged, closeAmount } from '../src/utils/close.ts';
const live = [{row: 2, imported_account: '4000', account: '4000 edited', name: 'Sales', mapping: 'Other income', debit: null, credit: '15000.25', identity_matches_import: false}];
test('close mapping draft uses current worksheet mapping and stable imported identity', () => {
  assert.deepEqual(liveMappingDraft(live), {4000: 'Other income'});
  assert.equal(mappingDraftChanged(live, {4000: 'Other income'}), false);
  assert.equal(mappingDraftChanged(live, {4000: 'Revenue'}), true);
});
test('invalid live amounts are never rendered as zero or as original CSV amounts', () => {
  assert.equal(closeAmount(null), 'Invalid amount');
  assert.equal(closeAmount('0'), '0');
  assert.equal(closeAmount('15000.25'), '15000.25');
});
