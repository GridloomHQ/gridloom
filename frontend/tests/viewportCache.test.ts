import assert from 'node:assert/strict';
import test from 'node:test';
import type {ViewportResponse} from '../src/types.ts';
import {bufferedViewport, requestCovers, ViewportCache, viewportBudget, viewportCovers, type ViewBounds} from '../src/utils/viewportCache.ts';

const bounds = (row:number,col:number,pinnedRows=0,pinnedColumns=0): ViewBounds => ({
  startRow:row,endRow:row+20,startCol:col,endCol:col+8,pinnedRows,pinnedColumns,
});
const response = (r:ViewBounds,generation=1):ViewportResponse => ({
  sheet_name:'Sheet1',start_row:r.startRow,end_row:r.endRow,start_col:r.startCol,end_col:r.endCol,
  total_rows:10_000_000,total_cols:100,cells:{},generation,
});

test('empty cells inside actual ranges are covered, but missing frozen strips are not', () => {
  const demand=bounds(9_000_000,20,2,3);
  const view=response(demand);
  const body={start_row:demand.startRow,end_row:demand.endRow,start_col:demand.startCol,end_col:demand.endCol};
  assert.equal(viewportCovers(view,demand),false);
  view.loaded_ranges=[body,{...body,start_row:0,end_row:1},{...body,start_col:0,end_col:2}];
  assert.equal(viewportCovers(view,demand),false,'missing corner does not count as loaded');
  view.loaded_ranges.push({start_row:0,end_row:1,start_col:0,end_col:2});
  assert.equal(viewportCovers(view,demand),true);
  assert.equal(viewportCovers(view,bounds(9_000_001,20,2,3)),false,'crossing a window needs data');
  assert.equal(viewportCovers(view,bounds(9_000_000,20,2,4)),false,'increased visible frozen prefix must load');
});

test('a forward buffer covers small scrolls and a far jump goes directly to its destination', () => {
  const old=bounds(50,10,2,3);
  const now=bounds(54,11,2,3);
  const forward=bufferedViewport(now,old,viewportBudget(8),256);
  const backward=bufferedViewport(now,bounds(60,11),viewportBudget(8),256);
  assert(forward.endRow-now.endRow > now.startRow-forward.startRow);
  assert(backward.endRow-now.endRow < now.startRow-backward.startRow);
  assert(requestCovers(forward,bounds(62,11,2,3)));
  assert(!requestCovers(forward,bounds(62,11,3,3)));
  const jump=bounds(9_000_000,20,2,3);
  const landing=bufferedViewport(jump,now,viewportBudget(8),256);
  assert(requestCovers(landing,jump));
  assert(landing.startRow>jump.startRow-45 && landing.endRow<jump.endRow+45);
  assert(!requestCovers(landing,now));
});

test('budget adapts to available device memory and dense payloads; the visible range is never sacrificed', () => {
  const now=bounds(200,10,2,3);
  const roomy=bufferedViewport(now,undefined,viewportBudget(8),256);
  const tight=bufferedViewport(now,undefined,viewportBudget(0.5),32_000);
  assert.equal(viewportBudget(undefined),4*1024*1024);
  assert.equal(viewportBudget(64),8*1024*1024);
  assert(requestCovers(tight,now));
  assert(roomy.endRow>tight.endRow);
  const end=bounds(9_999_990,93);
  const extent={total_rows:10_000_000,total_cols:100};
  const clipped=bufferedViewport(end,undefined,viewportBudget(8),256,extent);
  assert.equal(clipped.endRow,extent.total_rows-1);
  assert.equal(clipped.endCol,extent.total_cols-1);
});

test('cache LRU is bounded by bytes and discards previous generations and explicit clears', () => {
  const cache=new ViewportCache(1600);
  const a=bounds(0,0), b=bounds(50,0), c=bounds(100,0);
  cache.put(response(a));cache.put(response(b));
  assert(cache.find(a),'warm window can be revisited');
  cache.put(response(c));
  assert(cache.retainedBytes<=cache.budget);
  assert.equal(cache.find(b),undefined,'least recently used entry evicted');
  assert(cache.find(a));
  cache.put(response(c,2));
  assert.equal(cache.find(a),undefined,'new generation cannot reuse old windows');
  assert(cache.find(c));
  cache.clear();
  assert.equal(cache.find(c),undefined);
  assert.equal(cache.retainedBytes,0);
});
