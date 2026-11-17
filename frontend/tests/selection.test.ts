import { test } from 'node:test';
import assert from 'node:assert/strict';
import { normalizedRange, containsCell, encodeTsv, decodeTsv } from '../src/utils/selection.ts';
test('selection normalizes reversed dragging', () => {
 const range = normalizedRange({row: 4, col: 1}, {row: 1, col: 4});
 assert.deepEqual(range, {start:{row:1,col:1},end:{row:4,col:4}});
 assert.equal(containsCell(range,2,2),true); assert.equal(containsCell(range,0,2),false);
});
test('TSV preserves quoted multiline cells, blanks and formulas', () => {
 const rows = [['=A1+$B$2', 'tab\there', 'quote"yes'], ['line\nnext', '', '']];
 assert.deepEqual(decodeTsv(encodeTsv(rows)), rows);
 assert.deepEqual(decodeTsv('a\tb\r\nc\td\r\n'), [['a','b'],['c','d']]);
 assert.deepEqual(decodeTsv('a\tb\nc'), [['a','b'],['c','']]);
 assert.throws(() => decodeTsv('"missing'));
});

test('oversized clipboard is rejected before allocating a huge range', () => {
  assert.throws(() => decodeTsv('x\n'.repeat(10001)), /10,000/);
  assert.throws(() => decodeTsv('x\t'.repeat(10001)), /10,000/);
  assert.throws(() => decodeTsv('x'.repeat(8 * 1024 * 1024 + 1)), /8 MiB/);
});
