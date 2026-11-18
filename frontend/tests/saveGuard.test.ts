import {test} from 'node:test';
import assert from 'node:assert/strict';
import {saveBlocker} from '../src/utils/saveGuard.ts';
test('only committed content is eligible for save',()=>{
 assert.match(saveBlocker(true,false,false,[])!,/cell edit/);
 assert.match(saveBlocker(false,true,false,[])!,/cell edit/);
 assert.match(saveBlocker(false,false,true,[])!,/panel or dialog/);
 assert.match(saveBlocker(false,false,false,[{note:'pending',evidence:[]}])!,/retained review/);
 assert.match(saveBlocker(false,false,false,[{note:'',evidence:[],attachments:[{}]}])!,/retained review/);
 assert.equal(saveBlocker(false,false,false,[{note:'  ',evidence:[]}]),null);
});
