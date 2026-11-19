export const LEDGER_PAGE_ROWS=150;
export const LEDGER_ROW_PX=30;
export const LEDGER_BLOCK_ROWS=200000;
export function ledgerLocation(index:number){
  const base=Math.floor(index/LEDGER_BLOCK_ROWS)*LEDGER_BLOCK_ROWS;
  return {base,row:index-base};
}
export function ledgerOffset(base:number,row:number){
  return base+Math.max(0,Math.floor((row-28)/100)*100);
}
export function ledgerBlockRows(count:number,base:number){
  return Math.min(LEDGER_BLOCK_ROWS,Math.max(0,count-base));
}
