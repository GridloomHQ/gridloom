import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ROW_CANVAS_PX, segmentedRows } from '../src/utils/segmentedRows.ts';
import { paneIndices } from '../src/utils/frozenPanes.ts';

const H = 26, HEADER = 26, VIEWPORT = 600;
const ZERO = {originRow:0, scrollTop:0};

test('ordinary sheets retain native scroll coordinates and bounded viewport requests', () => {
  const geometry = segmentedRows(1000,H,VIEWPORT,HEADER,0);
  assert.equal(geometry.canvasHeight,1000*H+HEADER);
  assert.deepEqual(geometry.fromNative(0,312.5),{originRow:0,scrollTop:312.5});
  assert.equal(geometry.rowTop(12,ZERO),12*H+HEADER);
  assert.deepEqual(geometry.visible(ZERO,3),{startRow:0,endRow:25,visibleFrozenRows:0});
  assert.deepEqual(geometry.reveal(21,ZERO),ZERO);
});

test('row 10,000,000 and the last data row after a header are directly reachable', () => {
  const geometry = segmentedRows(10_000_001,H,VIEWPORT,HEADER,2);
  assert(geometry.canvasHeight <= ROW_CANVAS_PX);
  for (const row of [0,2,20,500_000,9_999_999,10_000_000]) {
    const scroll = geometry.reveal(row,ZERO);
    const rendered = geometry.visible(scroll,3);
    const top = geometry.rowTop(row,scroll) - scroll.scrollTop;
    assert(top >= (row<2 ? HEADER : HEADER+2*H));
    assert(top + H <= VIEWPORT);
    assert(paneIndices(rendered.startRow,rendered.endRow,2,rendered.visibleFrozenRows).includes(row));
    assert.equal(geometry.rowTop(0,scroll)-scroll.scrollTop,HEADER);
    assert.equal(geometry.rowTop(1,scroll)-scroll.scrollTop,HEADER+H);
    assert.equal(geometry.reveal(1,scroll),scroll);
    assert.deepEqual(geometry.reveal(row,scroll),scroll);
    assert(scroll.scrollTop >= 0 && scroll.scrollTop <= geometry.maxScrollTop);
  }
  const bottom = geometry.reveal(10_000_000,ZERO);
  assert.equal(bottom.scrollTop,geometry.maxScrollTop);
  assert.deepEqual(geometry.reveal(2,bottom),ZERO);
});

test('native rebases in both directions preserve exact logical scroll, IDs, and screen position', () => {
  const geometry = segmentedRows(10_000_000,H,VIEWPORT,HEADER,3);
  const deep = geometry.reveal(5_000_000,ZERO);
  for (const scrollTop of [88.5, geometry.maxScrollTop-71.25]) {
    const before = {originRow:deep.originRow,scrollTop};
    const after = geometry.fromNative(before.originRow,before.scrollTop);
    assert.notEqual(after.originRow,before.originRow);
    assert.equal(geometry.logicalScrollTop(after),geometry.logicalScrollTop(before));
    const range = geometry.visible(before,3);
    assert.deepEqual(geometry.visible(after,3),range);
    for (const row of [0,1,2,range.startRow,range.endRow]) {
      assert.equal(geometry.rowTop(row,after)-after.scrollTop,geometry.rowTop(row,before)-before.scrollTop);
    }
    assert.deepEqual(geometry.fromNative(after.originRow,after.scrollTop),after);
  }
});

test('frozen rows fill tiny viewports without huge pinned ranges or hidden scrolling', () => {
  const geometry = segmentedRows(10_000_000,H,400,HEADER,5_000_000);
  assert.deepEqual(geometry.visible(ZERO,3),{startRow:0,endRow:0,visibleFrozenRows:15});
  assert.equal(geometry.reveal(9_999_999,ZERO),ZERO);
  assert.equal(geometry.rowTop(14,ZERO),390);
});

test('continuous wheel scroll traverses multiple segments without losing logical pixels', () => {
  const geometry = segmentedRows(500_000,H,VIEWPORT,HEADER,2);
  let state = ZERO, rebases = 0;
  for (let i=0; i<2_000; i++) {
    const before = geometry.logicalScrollTop(state);
    const wheel = 9_999;
    const nativeTop = Math.min(geometry.maxScrollTop,state.scrollTop+wheel);
    const next = geometry.fromNative(state.originRow,nativeTop);
    if (next.originRow !== state.originRow) rebases++;
    assert.equal(geometry.logicalScrollTop(next),before+nativeTop-state.scrollTop);
    state=next;
  }
  assert(rebases > 10);
  assert.equal(geometry.visible(state,0).endRow,499_999);
});
