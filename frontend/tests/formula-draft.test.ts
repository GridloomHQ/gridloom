import {test} from 'node:test';
import assert from 'node:assert/strict';
import {draftReferences, referenceSlot, referenceText} from '../src/utils/formulaDraft.ts';

test('retains incomplete formulas, local/explicit same-sheet ranges and spill anchors; skips other sheets, strings, functions and structured refs',()=>{
  assert.deepEqual(draftReferences(`=SUM($A$1:B3,"F3",'Other sheet'!C2:D4,'Our sheet'!$E$6,Table[G7],LOG10(B4),A9#`,'Our sheet'),[
    {start:{row:0,col:0},end:{row:2,col:1}},
    {start:{row:5,col:4},end:{row:5,col:4}},
    {start:{row:3,col:1},end:{row:3,col:1}},
    {start:{row:8,col:0},end:{row:8,col:0}},
  ]);
  assert.deepEqual(draftReferences('not =A1','Sheet1'),[]);
});
test('detects a valid insertion slot without overriding ordinary caret editing',()=>{
  assert.deepEqual(referenceSlot('=SUM(,0)',5,5),{start:5,end:5});
  assert.deepEqual(referenceSlot('=A1:B4+2',1,6),{start:1,end:6});
  assert.equal(referenceSlot('="A1+',5,5),null);
  assert.equal(referenceSlot('=Table[A1+',10,10),null);
  assert.equal(referenceSlot('=7+18',4,4),null);
  assert.equal(referenceSlot('sum(',4,4),null);
});
test('generates normalized A1 rectangles and collapses singleton ranges',()=>{
  assert.equal(referenceText({row:2,col:27},{row:1,col:0}),'A2:AB3');
  assert.equal(referenceText({row:0,col:0},{row:0,col:0}),'A1');
});
