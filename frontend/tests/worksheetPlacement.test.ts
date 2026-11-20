import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parsePlacement } from '../src/utils/worksheetPlacement.ts';

test('placement requires a known existing worksheet and an explicit valid A1 anchor', () => {
  const sheets = ['Sheet1', 'Regional totals'];
  assert.equal(parsePlacement({sheetName:'', anchor:'A1'}, sheets), null);
  assert.equal(parsePlacement({sheetName:'missing', anchor:'A1'}, sheets), null);
  assert.equal(parsePlacement({sheetName:'Sheet1', anchor:''}, sheets), null);
  for (const anchor of ['A0', 'A1:B2', '123', 'B-2', 'A4294967296', 'XXXXXXXXXXXX2']) {
    assert.equal(parsePlacement({sheetName:'Sheet1', anchor}, sheets), null, anchor);
  }
  assert.deepEqual(parsePlacement({sheetName:'Regional totals', anchor:' aa10000001 '}, sheets),
    {sheetName:'Regional totals', row:10_000_000, col:26});
});
