/** Add a pinned origin without widening the requested/materialized main range. */
export function paneIndices(start:number,end:number,pinFirst:boolean):number[] {
  const indices=Array.from({length:Math.max(0,end-start+1)},(_,i)=>i+start);
  if(pinFirst && start>0)indices.unshift(0);
  return indices;
}
/** Logical cell offset excludes the fixed header. No useful scrolling is possible
 * when a frozen span occupies the entire viewport. Oversize cells align leading. */
export function revealAxis(offset:number,extent:number,scroll:number,available:number,frozen:number,pinned:boolean):number {
  if(pinned || available<=frozen)return scroll;
  const space=available-frozen;
  if(extent>space || offset<scroll+frozen)return Math.max(0,offset-frozen);
  if(offset+extent>scroll+available)return Math.max(0,offset+extent-available);
  return scroll;
}
export function paneOffset(index:number,offset:number,scroll:number,pinFirst:boolean):number {
  return pinFirst && index===0 ? scroll : offset;
}
export function paneLayer(row:number,col:number,top:boolean,left:boolean):number {
  return (top&&row===0?10:0)+(left&&col===0?10:0);
}
