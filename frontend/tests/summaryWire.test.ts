import test from 'node:test';
import assert from 'node:assert/strict';
import {api} from '../src/api.ts';
test('range summary sends explicit bounds and preserves numeric strings beyond JavaScript precision',async t=>{
 let request:unknown;
 t.mock.method(globalThis,'fetch',async(url,options)=>{
  assert.equal(url,'/api/ranges/summary');request=JSON.parse(String(options?.body));
  return new Response(JSON.stringify({selected_count:2,numeric_count:2,counts:{integer:2},sum:{value:'9007199254740994',quality:'exact',reason:null},average:{value:'4503599627370497',quality:'exact',reason:null}}));
 });
 const result=await api.rangeSummary('Accounts',{start:{row:2,col:3},end:{row:3,col:3}});
 assert.deepEqual(request,{sheet_name:'Accounts',start_row:2,start_col:3,end_row:3,end_col:3});
 assert.equal(result.sum.value,'9007199254740994');assert.equal(result.sum.quality,'exact');
});
