import {test} from 'node:test';
import assert from 'node:assert/strict';
import {cellPresentation} from '../src/utils/cellPresentation.ts';
test('formula view preserves exact formula text and evaluated value tooltip',()=>{
 const cell={formula:'=A1*$D$1',display_text:'9007199254740993'};
 assert.deepEqual(cellPresentation(cell,true),{text:'=A1*$D$1',title:'Formula: =A1*$D$1\nEvaluated value: 9007199254740993'});
 assert.deepEqual(cellPresentation(cell,false),{text:'9007199254740993',title:'9007199254740993'});
 assert.equal(cell.display_text,'9007199254740993');
});
test('literals, spill children, errors and blanks have honest presentation',()=>{
 assert.deepEqual(cellPresentation({formula:null,display_text:'42'},true),{text:'42',title:'42'});
 assert.deepEqual(cellPresentation({formula:'=1/0',display_text:'#DIV/0!'},true),{text:'=1/0',title:'Formula: =1/0\nEvaluated value: #DIV/0!'});
 assert.deepEqual(cellPresentation(undefined,true),{text:'',title:''});
});
