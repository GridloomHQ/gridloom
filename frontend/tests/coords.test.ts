import test from 'node:test';
import assert from 'node:assert/strict';
import { a1ToCoord, coordToA1 } from '../src/utils/coords.ts';
test('cell addresses support multi-letter columns and reject invalid or unsafe coordinates',()=>{
 assert.deepEqual(a1ToCoord(' aa200 '),{row:199,col:26});
 assert.equal(coordToA1(199,26),'AA200');
 for(const value of ['A0','A-1','A1:B2','Sheet1!A1','A90071992547409930','ZZZZZZZZZZZZZZZZ1',''])assert.equal(a1ToCoord(value),null,value);
});
