import assert from 'node:assert/strict';
import { test } from 'node:test';
import { barSpan, signedScale } from '../src/utils/chartScale.ts';

test('horizontal and vertical bars grow from a shared zero baseline', () => {
  const { min, max, fraction } = signedScale([[-5, 0, 5], [15]]);
  assert.deepEqual([min, max], [-5, 15]);
  const x = (v: number) => fraction(v) * 200;
  const y = (v: number) => 200 - x(v);
  assert.deepEqual(barSpan(-5, x), { start: 0, length: 50 });
  assert.deepEqual(barSpan(15, x), { start: 50, length: 150 });
  assert.deepEqual(barSpan(-5, y), { start: 150, length: 50 });
  assert.deepEqual(barSpan(15, y), { start: 0, length: 150 });
  assert.deepEqual(barSpan(0, x), { start: 50, length: 0 });
});

test('single-sign and zero-only domains stay finite and include zero', () => {
  assert.equal(signedScale([[-1, -2]]).fraction(0), 1);
  assert.equal(signedScale([[1, 2]]).fraction(0), 0);
  assert.equal(signedScale([[0, 0]]).fraction(0), 0);
});
