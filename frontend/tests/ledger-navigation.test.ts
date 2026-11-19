import assert from 'node:assert/strict';
import test from 'node:test';
import {ledgerLocation,ledgerOffset,ledgerBlockRows,LEDGER_PAGE_ROWS,LEDGER_ROW_PX,LEDGER_BLOCK_ROWS} from '../src/components/ledgerNavigation.ts';

test('two million rows use bounded block geometry and zero-based offsets',()=>{
  for(const [ordinal,expectedOffset] of [[1,0],[200000,199900],[200001,200000],[1048576,1048500],[1048577,1048500],[2000000,1999900]]){
    const {base,row}=ledgerLocation(ordinal-1);
    const offset=ledgerOffset(base,row);
    assert.equal(offset,expectedOffset);
    assert.equal(base+row,ordinal-1);
    assert(ordinal-1-offset>=0 && ordinal-1-offset<LEDGER_PAGE_ROWS);
    assert(ledgerBlockRows(2_000_000,base)*LEDGER_ROW_PX<=6_000_000);
  }
  const {base}=ledgerLocation(2_000_009);
  assert.equal(ledgerBlockRows(2_000_010,base),10);
  assert.equal(ledgerBlockRows(0,LEDGER_BLOCK_ROWS),0);
});
