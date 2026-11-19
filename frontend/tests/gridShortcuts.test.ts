import { test } from 'node:test';
import assert from 'node:assert/strict';
import { gridShortcut } from '../src/utils/gridShortcuts.ts';

test('worksheet shortcut distinguishes paste special from ordinary paste and ignores conflicting modifiers', () => {
  assert.equal(gridShortcut({key:'v',ctrlKey:true,altKey:true}), 'paste-special');
  assert.equal(gridShortcut({key:'V',metaKey:true,altKey:true}), 'paste-special');
  assert.equal(gridShortcut({key:'√',code:'KeyV',metaKey:true,altKey:true}), 'paste-special');
  assert.equal(gridShortcut({key:'v',ctrlKey:true}), null);
  assert.equal(gridShortcut({key:'v',ctrlKey:true,shiftKey:true}), null);
  assert.equal(gridShortcut({key:'v',altKey:true}), null);
  assert.equal(gridShortcut({key:'v',ctrlKey:true,altKey:true,shiftKey:true}), null);
});

test('fill right is available on both platforms without swallowing hard refresh', () => {
  assert.equal(gridShortcut({key:'r',ctrlKey:true}), 'fill-right');
  assert.equal(gridShortcut({key:'r',metaKey:true}), 'fill-right');
  assert.equal(gridShortcut({key:'r',metaKey:true,shiftKey:true}), null);
  assert.equal(gridShortcut({key:'d',ctrlKey:true}), 'fill-down');
  assert.equal(gridShortcut({key:'z',metaKey:true}), 'undo');
  assert.equal(gridShortcut({key:'Z',metaKey:true,shiftKey:true}), 'redo');
  assert.equal(gridShortcut({key:'y',ctrlKey:true}), 'redo');
});
