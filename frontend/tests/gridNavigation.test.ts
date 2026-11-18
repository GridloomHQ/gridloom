import { test } from 'node:test';
import assert from 'node:assert/strict';
import { gridNavigation } from '../src/utils/gridNavigation.ts';
const at = { row: 20, col: 8 };
const move = (key: string, flags = {}) => gridNavigation({ key, ...flags }, at, 100, 20, 15);
test('Enter and Tab move with Shift reversing instead of extending', () => {
  assert.deepEqual(move('Enter'), {row:21,col:8,extend:false});
  assert.deepEqual(move('Enter',{shiftKey:true}), {row:19,col:8,extend:false});
  assert.deepEqual(move('Tab'), {row:20,col:9,extend:false});
  assert.deepEqual(move('Tab',{shiftKey:true}), {row:20,col:7,extend:false});
});
test('Home and Control/Command Home navigate and optionally extend selection', () => {
  assert.deepEqual(move('Home'), {row:20,col:0,extend:false});
  assert.deepEqual(move('Home',{ctrlKey:true,shiftKey:true}), {row:0,col:0,extend:true});
  assert.deepEqual(move('Home',{metaKey:true}), {row:0,col:0,extend:false});
});
test('page and arrow movement use viewport height and preserve shift selection', () => {
  assert.deepEqual(move('PageDown',{shiftKey:true}), {row:35,col:8,extend:true});
  assert.deepEqual(move('PageUp'), {row:5,col:8,extend:false});
  assert.deepEqual(move('ArrowLeft',{shiftKey:true}), {row:20,col:7,extend:true});
});
test('movement clamps to sheet bounds, including tiny or empty grids', () => {
  assert.deepEqual(gridNavigation({key:'Tab'}, {row:0,col:0},1,1,0), {row:0,col:0,extend:false});
  assert.deepEqual(gridNavigation({key:'PageDown'}, {row:0,col:0},0,0,0), {row:0,col:0,extend:false});
  assert.deepEqual(gridNavigation({key:'Enter',shiftKey:true},{row:0,col:0},100,20,15), {row:0,col:0,extend:false});
});
test('unimplemented modified navigation and browser shortcuts are not intercepted', () => {
  assert.equal(move('ArrowRight',{ctrlKey:true}),null);
  assert.equal(move('PageDown',{altKey:true}),null);
  assert.equal(move('f',{ctrlKey:true}),null);
  assert.equal(move('F2'),null);
});
