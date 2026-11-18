export interface FunctionCompletion {start:number;end:number;prefix:string;}
const word=(c:string)=>/[A-Za-z0-9_.]/.test(c);
/** Find only a function-name token in formula code, never quoted/structured text. */
export function functionCompletion(input:string,start:number,end=start):FunctionCompletion|null {
  if(!input.trimStart().startsWith('=')||start!==end||!Number.isInteger(start)||start<0||start>input.length)return null;
  let quote='',brackets=0;
  for(let i=0;i<start;i++){
    const c=input[i];
    if(quote){if(c===quote){if(input[i+1]===quote){i++;}else quote='';}continue;}
    if(c==='"'||c==="'"){quote=c;continue;}
    if(c==='[')brackets++;else if(c===']')brackets=Math.max(0,brackets-1);
  }
  if(quote||brackets)return null;
  let first=start,last=start;
  while(first>0&&word(input[first-1]))first--;
  while(last<input.length&&word(input[last]))last++;
  const token=input.slice(first,last);
  const suffix=input.slice(last).trimStart();
  if(/^[A-Za-z]+[0-9]+$/.test(token)||suffix.startsWith('!')||suffix.startsWith('[')||suffix.startsWith('$'))return null;
  const prefix=input.slice(first,start);
  if(!/^[A-Za-z_][A-Za-z0-9_.]*$/.test(prefix))return null;
  let previous=first-1;while(previous>=0&&/\s/.test(input[previous]))previous--;
  if(previous<0||!/[=+\-*/^&(:,<>]/.test(input[previous]))return null;
  return {start:first,end:last,prefix:prefix.toUpperCase()};
}
export function applyFunctionCompletion(input:string,context:FunctionCompletion,name:string):{value:string;caret:number} {
  const rest=input.slice(context.end),open=rest.match(/^\s*\(/);
  const insertion=name+(open?'':'(');
  return {value:input.slice(0,context.start)+insertion+rest,caret:context.start+insertion.length+(open?open[0].length:0)};
}

/** Duplicate selection notifications must not undo keyboard menu navigation. */
export function completionEvent(previous:string|null,value:string,start:number|null,end:number|null,inputChanged:boolean):{key:string;update:boolean} {
  const key=JSON.stringify([value,start,end]);return {key,update:inputChanged||key!==previous};
}
