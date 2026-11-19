import type { LedgerSpec } from '../api';

export function sameLedgerSpec(left:LedgerSpec|null,right:LedgerSpec|null):boolean {
  if(!left||!right)return left===right;
  return left.connection===right.connection&&left.table===right.table&&left.date===right.date
    &&left.cents===right.cents&&left.from===right.from&&left.through===right.through
    &&left.account?.column===right.account?.column&&left.account?.value===right.account?.value;
}

function formatCents(cents:bigint):string {
  const abs=(cents<0n ? -cents : cents).toString().padStart(3,'0');
  return (cents<0n ? '-' : '') + abs.slice(0,-2) + '.' + abs.slice(-2);
}

export function compareLedgerBalance(balance:string,total:string,count:number) {
  const a=BigInt(balance),b=BigInt(total),d=a-b;
  return {balance:formatCents(a),total:formatCents(b),difference:formatCents(d),
    status:count===0?'No matching entries':d===0n?'Reconciled':'Difference found'};
}
