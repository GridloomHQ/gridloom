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

test('bulk operation dimensions reject enormous or invalid selections before allocation', async () => {
  const { boundedRangeDimensions } = await import('../src/utils/selection.ts');
  assert.deepEqual(boundedRangeDimensions({start:{row:0,col:0},end:{row:99,col:99}}),{rows:100,columns:100,count:10000});
  for (const end of [{row:100,col:99},{row:0xffffffff,col:0xffffffff},{row:Number.MAX_SAFE_INTEGER,col:0},{row:Infinity,col:0},{row:NaN,col:0},{row:-1,col:0},{row:0.5,col:0}]) {
    assert.throws(()=>boundedRangeDimensions({start:{row:0,col:0},end}));
  }
  assert.throws(()=>boundedRangeDimensions({start:{row:3,col:3},end:{row:2,col:4}}));
  assert.deepEqual(boundedRangeDimensions({start:{row:90000,col:90000},end:{row:90000,col:90000}}),{rows:1,columns:1,count:1});
});

test('clipboard rejects excessive aggregate field count across individually small rows', () => {
  assert.throws(()=>decodeTsv(('\t'.repeat(99)+'\n').repeat(101)),/10,000/);
  assert.throws(()=>decodeTsv(('x\t'.repeat(99)+'x\n').repeat(100)+'last'),/10,000/);
  const accepted=decodeTsv(('\t'.repeat(99)+'\n').repeat(100));
  assert.equal(accepted.length,100);assert.equal(accepted[0].length,100);
  assert.deepEqual(decodeTsv('one\ttwo'),[['one','two']]);
});
