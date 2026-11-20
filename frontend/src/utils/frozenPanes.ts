/** Render just the visible pinned prefix and the scrolling region, without duplicates. */
export function paneIndices(start:number,end:number,pinned:number,visiblePinned=pinned):number[] {
  const pinnedEnd=Math.max(0,Math.min(visiblePinned,pinned));
  const bodyStart=Math.max(start,pinned);
  return [
    ...Array.from({length:pinnedEnd},(_,i)=>i),
    ...Array.from({length:Math.max(0,end-bodyStart+1)},(_,i)=>i+bodyStart),
  ];
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
export function paneOffset(index:number,offset:number,scroll:number,pinned:number):number {
  return index<pinned ? scroll+offset : offset;
}
export function paneLayer(row:number,col:number,top:number,left:number):number {
  return (row<top?10:0)+(col<left?10:0);
}
