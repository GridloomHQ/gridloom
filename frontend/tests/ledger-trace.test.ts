import assert from 'node:assert/strict';
import test from 'node:test';
import {compareLedgerBalance,sameLedgerSpec} from '../src/components/ledgerTrace.ts';

test('reconciliation preserves exact cents beyond the Javascript safe number range',()=>{
  assert.deepEqual(compareLedgerBalance('9007199254741010','9007199254741009',2),{
    balance:'90071992547410.10',total:'90071992547410.09',difference:'0.01',status:'Difference found',
  });
  assert.equal(compareLedgerBalance('-1','0',1).difference,'-0.01');
  assert.equal(compareLedgerBalance('9007199254741009','9007199254741009',2).status,'Reconciled');
  assert.equal(compareLedgerBalance('0','0',0).status,'No matching entries');
});
test('a saved link requires the same source, columns, account and period',()=>{
  const spec={connection:'GL',table:'entries',date:'date',cents:'cents',from:'2026-11-01',through:'2026-11-30',account:{column:'account',value:'001'}};
  assert(sameLedgerSpec(spec,structuredClone(spec)));
  assert(!sameLedgerSpec(spec,{...spec,account:{column:'account',value:'1'}}));
  assert(!sameLedgerSpec(spec,{...spec,through:'2026-11-29'}));
  assert(!sameLedgerSpec(spec,{...spec,connection:'OTHER'}));
  assert(!sameLedgerSpec(spec,null));
});
