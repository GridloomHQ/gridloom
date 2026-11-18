import test from 'node:test';
import assert from 'node:assert/strict';
import {cycleReference} from '../src/utils/referenceCycle.ts';
const caret=(s:string)=>{const at=s.indexOf('|');return cycleReference(s.replace('|',''),at,at);};
test('cycles all modes and returns a precise token selection for repeated F4',()=>{
 let text='=A1*2',start=3,end=3;
 for(const expected of ['=$A$1*2','=A$1*2','=$A1*2','=A1*2']){
   const next=cycleReference(text,start,end)!;assert.equal(next.value,expected);
   text=next.value;start=next.selectionStart;end=next.selectionEnd;
   assert.equal(start,1);assert.equal(end,text.indexOf('*'));
 }
});
test('caret within or at either boundary changes only the single reference',()=>{
 for(const formula of ['=|aa123+B4','=a|a123+B4','=aa1|23+B4','=aa123|+B4'])assert.equal(caret(formula)?.value,'=$aa$123+B4');
 assert.equal(caret('=SUM(A1:B|20)')?.value,'=SUM(A1:$B$20)');
 assert.equal(caret('=SUM(A1|,')?.value,'=SUM($A$1,');
 assert.equal(caret('=A1|#')?.value,'=$A$1#');
 assert.equal(caret("='Forecast 2027'!B|2")?.value,"='Forecast 2027'!$B$2");
 assert.equal(caret("='O''Brien A1'!b2|")?.value,"='O''Brien A1'!$b$2");
 assert.equal(caret('=Sheet1!A|1')?.value,'=Sheet1!$A$1');
});
test('strings, escaped strings, names, functions, scientific notation and structured references are untouched',()=>{
 for(const formula of ['="A|1"','=CONCAT("say ""A|1""", B2)',"='Sheet1|'!A1",'=Sheet1|!A1','=Table1|[Amount]','=Table1[A|1]','=Table1[[#Headers],[A|1]]','=LOG10|(A1)','=LOG10| (A1)','=A1_name|','=1E10|','=1.2e3|','=1e+3|','=#REF!A1|'])assert.equal(caret(formula),null,formula);
 assert.equal(caret('=SQLVALUE("db", "SELECT A1 FROM data", B|2)')?.value,'=SQLVALUE("db", "SELECT A1 FROM data", $B$2)');
});
test('ambiguous, partial and invalid selections do not edit drafts',()=>{
 for(const [text,start,end] of [['=A1+B2',1,6],['=A1:B2',1,6],['=AA12',2,4],['A1',0,2],['=A1',-1,2],['=A1',1,99],['=A1',2,1],['=A1',1.5,1.5]] as [string,number,number][])assert.equal(cycleReference(text,start,end),null,text);
 for(const formula of ['="broken+A|1',"='broken!A|1",'=Table1[broken]+A|1[','=$$A|1','=A$1$|','=A0|','=A4294967296|','=ZZZZZZZZZZ1|','=éA1|','=A1|]'])assert.equal(caret(formula),null,formula);
});
test('UTF-16 positions after quoted Unicode remain correct and core limits are respected',()=>{
 const text='=CONCAT("😀",A1)',start=text.indexOf('A1');const result=cycleReference(text,start,start+2)!;
 assert.equal(result.value,'=CONCAT("😀",$A$1)');assert.equal(result.selectionStart,start);assert.equal(result.selectionEnd,start+4);
 assert.equal(caret('=A4294967295|')?.value,'=$A$4294967295');
});
