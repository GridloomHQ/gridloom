import { test } from 'node:test';
import assert from 'node:assert/strict';
import { paneIndices, revealAxis, paneOffset, paneLayer } from '../src/utils/frozenPanes.ts';
test('pinned strips keep distant virtual ranges bounded without duplicating zero',()=>{
 assert.deepEqual(paneIndices(100,102,true),[0,100,101,102]);
 assert.deepEqual(paneIndices(0,2,true),[0,1,2]);
 assert.deepEqual(paneIndices(100,102,false),[100,101,102]);
});
test('normal cells reveal beyond frozen top/left bands',()=>{
 assert.equal(revealAxis(110,110,500,600,110,false),0);
 assert.equal(revealAxis(500,110,500,600,110,false),390);
 assert.equal(revealAxis(1000,110,500,600,110,false),510);
 assert.equal(revealAxis(700,110,500,600,110,false),500);
});
test('pinned cells never change scroll and tiny viewports remain stable',()=>{
 assert.equal(revealAxis(0,110,500,600,110,true),500);
 assert.equal(revealAxis(1000,110,500,100,110,false),500);
 const next=revealAxis(1000,600,500,300,110,false);
 assert.equal(next,890);assert.equal(revealAxis(1000,600,next,300,110,false),next);
 assert.equal(revealAxis(0,26,0,20,0,false),0);
});
test('pane positions and layering distinguish the pinned intersection',()=>{
 assert.equal(paneOffset(0,0,500,true),500);assert.equal(paneOffset(1,110,500,true),110);
 assert.equal(paneOffset(0,0,500,false),0);
 assert.equal(paneLayer(0,0,true,true),20);assert.equal(paneLayer(0,8,true,true),10);
 assert.equal(paneLayer(9,0,true,true),10);assert.equal(paneLayer(9,8,true,true),0);
});
