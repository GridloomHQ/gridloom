export interface ReferenceCycleResult {
  value: string;
  selectionStart: number;
  selectionEnd: number;
}
interface ReferenceToken {start:number;end:number;column:string;row:string;columnLocked:boolean;rowLocked:boolean;}

/** Cycle a single A1 reference in a formula draft. Offsets are DOM UTF-16 offsets.
 * This deliberately is not a formula parser: incomplete parentheses are useful
 * while editing, but ambiguous selections and unsafe lexical contexts are no-ops.
 * Never rewrite quoted text, sheet names, structured columns or numeric tokens.
 */
export function cycleReference(input:string, selectionStart:number, selectionEnd:number):ReferenceCycleResult|null {
  if(!input.trimStart().startsWith('=') || !Number.isInteger(selectionStart) || !Number.isInteger(selectionEnd)
    || selectionStart<0 || selectionEnd<selectionStart || selectionEnd>input.length)return null;
  const tokens:ReferenceToken[]=[];
  let i=input.indexOf('=')+1;
  while(i<input.length){
    const char=input[i];
    if(char==='"'||char==="'"){
      const quote=char;i++;let closed=false;
      while(i<input.length){
        if(input[i]===quote){i++;if(input[i]===quote){i++;continue;}closed=true;break;}
        i++;
      }
      if(!closed)return null;
      continue;
    }
    if(char==='['){
      let depth=1;i++;
      while(i<input.length&&depth){if(input[i]==='[')depth++;else if(input[i]===']')depth--;i++;}
      if(depth)return null;
      continue;
    }
    if(char===']')return null;
    if(/[A-Za-z0-9_.$]/.test(char)){
      const start=i;
      while(i<input.length&&/[A-Za-z0-9_.$]/.test(input[i]))i++;
      const token=input.slice(start,i),match=/^(\$?)([A-Za-z]+)(\$?)([0-9]+)$/.exec(token);
      if(!match)continue;
      const next=input.slice(i).match(/^\s*(.)/)?.[1];
      if(next==='!'||next==='['||next==='(')continue;
      // Match the core's u32 A1 limits without invoking lossy large-integer math.
      let column=0;
      for(const letter of match[2].toUpperCase()){
        column=column*26+letter.charCodeAt(0)-64;
        if(column>0xffffffff)break;
      }
      const row=Number(match[4]);
      if(column>0xffffffff||row<1||row>0xffffffff||!Number.isInteger(row))continue;
      tokens.push({start,end:i,column:match[2],row:match[4],columnLocked:!!match[1],rowLocked:!!match[3]});
      continue;
    }
    if(char==='#'){
      // Skip error literals; a spill suffix is just a lone # after a reference.
      i++;while(i<input.length&&/[A-Za-z0-9/!?]/.test(input[i]))i++;
      continue;
    }
    if(!/[\s=+\-*/^&%(),:;<>!{}]/.test(char))return null;
    i++;
  }
  const matches=tokens.filter(t=>selectionStart===selectionEnd
    ?selectionStart>=t.start&&selectionStart<=t.end
    :selectionStart===t.start&&selectionEnd===t.end);
  if(matches.length!==1)return null;
  const token=matches[0];
  const locks=!token.columnLocked&&!token.rowLocked?[true,true]
    :token.columnLocked&&token.rowLocked?[false,true]
    :!token.columnLocked&&token.rowLocked?[true,false]:[false,false];
  const replacement=`${locks[0]?'$':''}${token.column}${locks[1]?'$':''}${token.row}`;
  return {value:input.slice(0,token.start)+replacement+input.slice(token.end),selectionStart:token.start,selectionEnd:token.start+replacement.length};
}
