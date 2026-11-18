import test from 'node:test';
import assert from 'node:assert/strict';
import { api } from '../src/api.ts';

test('dependency API normalizes range extents for on-grid highlighting', async t => {
  t.mock.method(globalThis, 'fetch', async () => new Response(JSON.stringify({
    sheet_name: 'Trial balance', coord: {row:2,col:8}, warnings: [], truncated: false,
    upstream: [{id:'amounts',kind:'range',label:'M2:M5',sheet_name:'Trial balance',coord:{row:1,col:12},
      range:{start_row:1,start_col:12,end_row:4,end_col:12},formula:null,source_name:null}],
    downstream: [{id:'consumer',kind:'cell',label:'I10',sheet_name:'Report',coord:{row:9,col:8},
      range:null,formula:"='Trial balance'!I3",source_name:null}],
  })));
  const result = await api.navigateDependencies('Trial balance',2,8);
  assert.deepEqual(result.upstream[0].range,{start:{row:1,col:12},end:{row:4,col:12}});
  assert.equal(result.downstream[0].range,null);
});
