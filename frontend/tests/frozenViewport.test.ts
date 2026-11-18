import test from 'node:test';
import assert from 'node:assert/strict';
import {isCellLoaded} from '../src/utils/frozenViewport.ts';
const main={sheet_name:'Sheet1',start_row:200,start_col:40,end_row:230,end_col:50,generation:1,total_rows:1000,total_cols:100,cells:{}};
test('only explicitly loaded panes permit editing; distant gaps remain unloaded',()=>{
 const view={...main,loaded_ranges:[main,{start_row:0,end_row:0,start_col:40,end_col:50},{start_row:200,end_row:230,start_col:0,end_col:0},{start_row:0,end_row:0,start_col:0,end_col:0}]};
 assert.equal(isCellLoaded(view,0,45),true);assert.equal(isCellLoaded(view,210,0),true);assert.equal(isCellLoaded(view,0,0),true);
 assert.equal(isCellLoaded(view,20,20),false);assert.equal(isCellLoaded(view,210,45),true);
});
test('ordinary viewports retain loaded bounds and absent viewports are not empty cells',()=>{
 assert.equal(isCellLoaded(main,210,45),true);assert.equal(isCellLoaded(main,0,0),false);assert.equal(isCellLoaded(null,0,0),false);
});
