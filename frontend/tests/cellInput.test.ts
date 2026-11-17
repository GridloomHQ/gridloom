import test from 'node:test';
import assert from 'node:assert/strict';
import { editableCellInput } from '../src/utils/cellInput.ts';
test('editing preserves exact literals instead of regenerating from evaluated numeric values', () => {
  assert.equal(editableCellInput({ raw_input: '00123', raw_value: { type: 'Integer', value: 123 } }), '00123');
  assert.equal(editableCellInput({ raw_input: ' 12 ', raw_value: { type: 'Integer', value: 12 } }), ' 12 ');
  assert.equal(editableCellInput({ raw_input: '', raw_value: { type: 'Blank' } }), '');
  assert.equal(editableCellInput({ formula: '=A1', raw_value: { type: 'Integer', value: 12 } }), '=A1');
  assert.equal(editableCellInput({ raw_value: { type: 'Text', value: '001' } }), '001');
  assert.equal(editableCellInput(), '');
});
