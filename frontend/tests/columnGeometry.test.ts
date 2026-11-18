import { test } from 'node:test';
import assert from 'node:assert/strict';
import { columnGeometry, clampColumnWidth } from '../src/utils/columnGeometry.ts';
test('default widths and exact boundaries use half-open cells',()=>{
 const g=columnGeometry(100,{});assert.equal(g.totalWidth,11000);
 assert.equal(g.offset(3),330);assert.equal(g.width(3),110);
 assert.equal(g.atPixel(109.9),0);assert.equal(g.atPixel(110),1);
 assert.equal(g.atPixel(-1),0);assert.equal(g.atPixel(11000),99);
});
test('sparse custom widths produce cumulative offsets and inverse lookup',()=>{
 const g=columnGeometry(100,{0:48,3:600,99:250});
 assert.equal(g.offset(1),48);assert.equal(g.offset(3),268);assert.equal(g.offset(4),868);
 assert.equal(g.atPixel(267),2);assert.equal(g.atPixel(268),3);assert.equal(g.atPixel(867),3);assert.equal(g.atPixel(868),4);
 assert.equal(g.totalWidth,11000-62+490+140);
 for(let c=0;c<100;c++){assert.equal(g.atPixel(g.offset(c)),c);assert.equal(g.atPixel(g.offset(c)+g.width(c)-0.01),c);}
});
test('distant columns do not require a dense width array',()=>{
 const g=columnGeometry(1_000_000,{0:200,999_998:48});
 assert.equal(g.offset(999_999),999_999*110+90-62);
 assert.equal(g.atPixel(g.offset(999_998)+47),999_998);
 assert.equal(g.atPixel(Infinity),999_999);
});
test('invalid widths and bounds are normalized defensively',()=>{
 const g=columnGeometry(3,{'-1':400,0:NaN,1:2,2:900,3:200,1.5:88});
 assert.equal(g.width(0),110);assert.equal(g.width(1),110);assert.equal(g.width(2),110);
 assert.equal(g.offset(-2),0);assert.equal(g.offset(9),330);assert.equal(g.atPixel(NaN),0);
 assert.equal(columnGeometry(0,{}).totalWidth,0);assert.equal(columnGeometry(0,{}).atPixel(10),0);
 assert.equal(clampColumnWidth(222.7),223);assert.equal(clampColumnWidth(Infinity),110);
});
