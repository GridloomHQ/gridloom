import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const recovery = mkdtempSync(path.join(tmpdir(), 'gridloom-sheet-tabs-'));
const server = spawn(process.env.GRIDLOOM_SERVER ?? path.join(root, 'target/debug/gridloom-server'),
  ['--port', '0', '--no-samples'],
  {cwd: root, env: {...process.env, GRIDLOOM_RECOVERY_DIR: recovery}});
let log = '', browser;
server.stdout.on('data', chunk => log += chunk);
server.stderr.on('data', chunk => log += chunk);
server.on('error', error => log += error.message);
try {
  let base;
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline && server.exitCode === null
    && !(base = log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/)?.[1])) {
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  assert(base, log);
  browser = await chromium.launch({headless: true, executablePath: process.env.CHROMIUM_PATH});
  const page = await browser.newPage({viewport:{width:960,height:600}});
  await page.goto(base);
  const tab = name => page.getByRole('button', {name, exact: true});
  const item = name => page.getByRole('menuitem', {name, exact: true});
  const grid = name => page.getByRole('grid', {name: `${name} worksheet`});
  await grid('Sheet1').waitFor();
  await tab('Sheet1').click({button:'right'});
  assert(await item('Delete worksheet').isDisabled());
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('menu').count(), 0);
  assert.equal(await tab('Sheet1').evaluate(el => el === document.activeElement), true);

  page.once('dialog', dialog => dialog.accept('Input'));
  await tab('Add worksheet').click();
  await grid('Input').waitFor();
  await tab('Sheet1').click({button:'right'});
  page.once('dialog', dialog => dialog.accept('Report'));
  await item('Rename worksheet').click();
  await tab('Report').waitFor();
  await grid('Input').waitFor();

  await tab('Report').focus();
  await page.keyboard.press('Shift+F10');
  assert(await item('Delete worksheet').isVisible());
  await page.keyboard.press('ArrowDown');
  assert.equal(await item('Delete worksheet').evaluate(el => el === document.activeElement), true);
  page.once('dialog', dialog => dialog.dismiss());
  await page.keyboard.press('Enter');
  assert(await tab('Report').isVisible());

  await tab('Input').click({button:'right'});
  page.once('dialog', dialog => dialog.accept());
  await item('Delete worksheet').click();
  await grid('Report').waitFor();
  assert.equal(await tab('Input').count(), 0);
  await tab('Report').click({button:'right'});
  assert(await item('Delete worksheet').isDisabled());
  console.log('PASS: worksheet tab context menu, focus, rename, canceled delete, confirmed delete, and last-sheet guard');
} finally {
  await browser?.close();
  if (server.exitCode === null && server.signalCode === null && server.pid) {
    const exited = new Promise(resolve => server.once('exit',resolve));
    if (server.kill()) await exited;
  }
  rmSync(recovery,{recursive:true,force:true});
}
