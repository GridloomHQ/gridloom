import {test} from 'node:test';
import assert from 'node:assert/strict';
import {api,getPendingMutations,subscribeMutations} from '../src/api.ts';
test('pending mutation guard includes modal writes and releases on failures',async()=>{
 const original=globalThis.fetch;let release!:(value:Response)=>void;
 const events:number[]=[];const stop=subscribeMutations(()=>events.push(getPendingMutations()));
 globalThis.fetch=()=>new Promise(resolve=>{release=resolve;});
 try {
  const pending=api.setCell('Sheet1',0,0,'1');assert.equal(getPendingMutations(),1);
  release(new Response('failure',{status:500}));await assert.rejects(pending,/failure/);assert.equal(getPendingMutations(),0);assert.deepEqual(events,[1,0]);
  const review=api.recordAttachedReview({review:{} as never,attachments:[]});assert.equal(getPendingMutations(),1,'Review-panel writes must block workbook replacement');release(new Response('{}',{status:200}));await review;assert.equal(getPendingMutations(),0);
  const read=api.rangeSummary('Sheet1',{start:{row:0,col:0},end:{row:0,col:0}});assert.equal(getPendingMutations(),0);release(new Response('{}',{status:200}));await read;
 }finally{globalThis.fetch=original;stop();}
});
