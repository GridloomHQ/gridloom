import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { browserFunctions, FUNCTION_CATEGORIES, functionHelp } from '../src/utils/functionCatalog.ts';

test('each engine function and alias has accurate-name syntax, argument help, output and example', () => {
  const source = readFileSync(new URL('../../crates/gridloom-core/src/functions/mod.rs', import.meta.url), 'utf8');
  const dispatch = source.split('define_functions! { (args, ctx) {')[1];
  assert(dispatch);
  const supported = [...dispatch.matchAll(/^\s*((?:"[A-Z.]+")(?:\s*\|\s*"[A-Z.]+")*)\s*=>/gm)]
    .flatMap(m => [...m[1].matchAll(/"([A-Z.]+)"/g)].map(name => name[1]));
  assert(supported.length > 70);
  for (const name of supported) {
    const info = functionHelp(name);
    assert(info, `missing help: ${name}`);
    assert(FUNCTION_CATEGORIES.includes(info.category));
    assert(info.syntax.startsWith(`=${name}(`));
    assert(info.example.startsWith(`=${name}(`));
    assert(info.description && info.returns);
    assert(info.arguments.every(argument => !!argument.detail), `missing input detail: ${name}`);
    assert(info.aliases.every(alias => supported.includes(alias) && functionHelp(alias)?.aliases.includes(name)));
  }
  assert.deepEqual(functionHelp('NOW')?.arguments, []);
});

test('search combines words and category, and does not advertise frontend-only names', () => {
  const names = ['SUM', 'SUMPRODUCT', 'STDEV', 'STDEV.S', 'XLOOKUP', 'SOMEDAY'];
  assert.deepEqual(browserFunctions(names, ' standard SAMPLE ', 'Statistics'), ['STDEV', 'STDEV.S']);
  assert.deepEqual(browserFunctions(names, 'stdev.s', 'Statistics'), ['STDEV', 'STDEV.S']);
  assert.deepEqual(browserFunctions(names, 'sum', 'SQL'), []);
  assert.deepEqual(browserFunctions(names, 'SQLVALUE', 'All'), []);
  assert.deepEqual(browserFunctions(names, '', 'Other'), ['SOMEDAY']);
  assert.deepEqual(browserFunctions(names, 'find match', 'All'), ['XLOOKUP']);
});
