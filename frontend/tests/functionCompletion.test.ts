import {test} from 'node:test';
import assert from 'node:assert/strict';
import {functionCompletion,applyFunctionCompletion} from '../src/utils/functionCompletion.ts';
test('completion finds dotted and nested function tokens at the caret',()=>{
 assert.deepEqual(functionCompletion('=stdev.s',8),{start:1,end:8,prefix:'STDEV.S'});
 const text='=IF(A1>0,SU(A1:A3),0)';const c=functionCompletion(text,11)!;
 assert.deepEqual(c,{start:9,end:11,prefix:'SU'});
 assert.deepEqual(applyFunctionCompletion(text,c,'SUM'),{value:'=IF(A1>0,SUM(A1:A3),0)',caret:13});
});
test('replacement preserves suffix and existing parentheses',()=>{
 const text='=ROUND(A1)+SUX (B1)';const c=functionCompletion(text,13)!;
 assert.deepEqual(applyFunctionCompletion(text,c,'SUM'),{value:'=ROUND(A1)+SUM (B1)',caret:16});
 assert.deepEqual(applyFunctionCompletion('=RO',functionCompletion('=RO',3)!,'ROUND'),{value:'=ROUND(',caret:7});
});
test('strings, escaped quotes, quoted sheets, table columns and references are excluded',()=>{
 for(const text of ['="SU','="a""SU',"='SU",'=[Book]Sheet!SU','=Table[SU','=Table[[SU','=Sheet!SU','=12SU'])assert.equal(functionCompletion(text,text.length),null,text);
 assert.equal(functionCompletion('plain SU',8),null);assert.equal(functionCompletion('=SUM',1,4),null);
 assert.equal(functionCompletion('=SUM',99),null);
});

test('caret inside references and qualifier names cannot replace the surrounding syntax',()=>{
 assert.equal(functionCompletion('=SU!A1',3),null);
 assert.equal(functionCompletion('=SUM [Amt]',4),null);
 assert.equal(functionCompletion('=SUM1',4),null);
 assert.equal(functionCompletion('=SUM$1',4),null);
 assert.equal(functionCompletion('=A$1',2),null);
 assert.equal(functionCompletion('=ROUND99',4),null);
});
