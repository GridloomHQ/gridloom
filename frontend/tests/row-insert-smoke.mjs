import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const recovery = mkdtempSync(path.join(tmpdir(), 'gridloom-row-smoke-'));
const binary = process.env.GRIDLOOM_SERVER ?? path.join(root, 'target/debug/gridloom-server');
const server = spawn(binary, ['--port', '0', '--no-samples'], {
  cwd: root, env: {...process.env, GRIDLOOM_RECOVERY_DIR: recovery},
});
let log = '', browser;
server.stdout.on('data', chunk => log += chunk);
server.stderr.on('data', chunk => log += chunk);
server.on('error', error => log += error.message);
try {
  let base;
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline && server.exitCode === null && !(base = log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/)?.[1])) {
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  assert(base, log);
  browser = await chromium.launch({headless: true, executablePath: process.env.CHROMIUM_PATH});
  const page = await browser.newPage({viewport:{width:1440,height:1000}});
  const post = async (route, data) => {
    const response = await page.request.post(base + '/api/' + route, {data});
    assert(response.ok(), await response.text());
    return response.json();
  };
  await post('ranges/edit', {sheet_name:'Sheet1',row:0,col:0,values:[['=A2+1'],['41']]});
  const range = {start_row:0,start_col:0,end_row:2,end_col:0};
  const capture = () => post('ranges/capture', {sheet_name:'Sheet1',range});
  const before = await capture();
  await page.goto(base);
  const cell = addr => page.getByRole('gridcell', {name:addr,exact:true});
  await cell('A2').click();
  const insert = page.getByRole('button', {name:'Insert rows above',exact:true});
  let confirmText;
  page.once('dialog', async dialog => {
    confirmText = dialog.message();
    await dialog.dismiss();
  });
  await insert.click();
  assert.match(confirmText, /clears Undo\/Redo history/);
  assert.deepEqual(await capture(),before);
  page.once('dialog', dialog => dialog.accept());
  await insert.click();
  await page.getByText('Inserted one row above row 2',{exact:true}).waitFor();
  await cell('A3').filter({hasText:'41'}).waitFor();
  const cells = await post('cells/viewport', {sheet_name:'Sheet1',...range,overscan:0});
  assert.equal(cells.cells.A1.formula,'=A3+1');
  assert.equal(cells.cells.A1.display_text,'42');
  assert.equal(cells.cells.A2?.display_text ?? '', '');
  console.log('PASS: cancellation and confirmed insertion preserve the calculation and move its reference.');
} finally {
  await browser?.close();
  if (server.exitCode === null && server.signalCode === null && server.pid) {
    const exited = new Promise(resolve => server.once('exit',resolve));
    if (server.kill()) await exited;
  }
  rmSync(recovery,{recursive:true,force:true});
}
