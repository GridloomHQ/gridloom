import {a1ToCoord, coordToA1} from './coords.ts';
import type {GridRange} from './selection.ts';
type Coord = GridRange['start'];

const word = /[A-Za-z0-9_.$]/;
const ref = /^\$?[A-Za-z]+\$?[1-9][0-9]*$/;
export const referenceColors = ['#2563eb','#c026d3','#059669','#ea580c','#7c3aed','#0891b2'];
export function referenceRange(a: Coord, b: Coord): GridRange {
  return {start:{row:Math.min(a.row,b.row),col:Math.min(a.col,b.col)},end:{row:Math.max(a.row,b.row),col:Math.max(a.col,b.col)}};
}
export function referenceText(a:Coord,b:Coord):string {
  const {start,end}=referenceRange(a,b);
  return start.row===end.row&&start.col===end.col?coordToA1(start.row,start.col):`${coordToA1(start.row,start.col)}:${coordToA1(end.row,end.col)}`;
}

function tokens(input:string) {
  const result:{text:string;start:number;end:number;kind:'word'|'quoted'|'opaque'|'punct';unfinished?:boolean}[]=[];
  let i=input.indexOf('=')+1;
  while(i<input.length) {
    const start=i,c=input[i++];
    if (/\s/.test(c)) continue;
    if(c==='"'||c==="'") {
      let closed=false;while(i<input.length){if(input[i++]===c){if(input[i]!==c){closed=true;break;}i++;}}
      result.push({text:input.slice(start,i),start,end:i,kind:c==="'"?'quoted':'opaque',unfinished:!closed});
    } else if(c==='[') {
      let depth=1;
      while(i<input.length&&depth){const next=input[i++];if(next==='[')depth++;if(next===']')depth--;}
      result.push({text:input.slice(start,i),start,end:i,kind:'opaque',unfinished:depth>0});
    } else if(c==='#') {
      // An error literal is opaque. A spill # with nothing after it is harmless.
      while(i<input.length&&/[A-Za-z0-9/!?]/.test(input[i]))i++;
      result.push({text:input.slice(start,i),start,end:i,kind:'opaque'});
    } else if(word.test(c)) {
      while(i<input.length&&word.test(input[i]))i++;
      result.push({text:input.slice(start,i),start,end:i,kind:'word'});
    } else result.push({text:c,start,end:i,kind:'punct'});
  }
  return result;
}

export function draftReferences(input:string,sheetName:string):GridRange[] {
  if(!input.trimStart().startsWith('='))return [];
  const chunks=tokens(input), ranges:GridRange[]=[];
  const coordinate=(i:number)=>{const t=chunks[i];return t?.kind==='word'&&ref.test(t.text)&&!['!','('].includes(chunks[i+1]?.text)&&!chunks[i+1]?.text.startsWith('[')?a1ToCoord(t.text.replace(/\$/g,'')):null;};
  for(let i=0;i<chunks.length;i++){
    const start=coordinate(i);
    if(!start)continue;
    const sheet=chunks[i-1]?.text==='!'?chunks[i-2]?.text:null;
    const end=chunks[i+1]?.text===':'?coordinate(i+2):null;
    if(!sheet||sheet.replace(/^'|'$/g,'').replace(/''/g,"'").toLowerCase()===sheetName.toLowerCase())ranges.push(referenceRange(start,end??start));
    if(end)i+=2;
  }
  return ranges;
}

/** Return an insertion site only where an operand, or a selected A1 range, can go. */
export function referenceSlot(input:string,start:number,end:number):{start:number;end:number}|null {
  if(!input.trimStart().startsWith('=')||start<=input.indexOf('=')||end<start||end>input.length)return null;
  if(start!==end){
    const part=input.slice(start,end).split(':');
    if(part.length>2||!part.every(s=>ref.test(s.trim())))return null;
  }
  const chunks=tokens(input);
  if(chunks.some(t=>t.start<start&&start<t.end||t.start<end&&end<t.end))return null;
  if(chunks.some(t=>t.end===start&&t.unfinished))return null;
  const before=input.slice(0,start).trimEnd().slice(-1),after=input.slice(end).trimStart()[0];
  return before && /[=+\-*/^&(,;<>:{]/.test(before) && (!after || /[+\-*/^&),;<>:%}]/.test(after))?{start,end}:null;
}
