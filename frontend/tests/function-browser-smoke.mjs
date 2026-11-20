import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const fixture = mkdtempSync(path.join(os.tmpdir(), 'gridloom-function-browser-'));
const server = spawn(process.env.GRIDLOOM_SERVER || path.join(root, 'target/debug/gridloom-server'),
  ['--port', '0'], {
    cwd: root, env: { ...process.env, XDG_DATA_HOME: fixture,
      GRIDLOOM_FRONTEND_DIR: path.join(root, 'frontend/dist') },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
let log = '', browser;
server.stdout.on('data', data => { log += data; });
server.stderr.on('data', data => { log += data; });
try {
  const base = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => { clearInterval(poll); reject(new Error(`Server timeout: ${log}`)); }, 30000);
    const poll = setInterval(() => {
      const entry = log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);
      if (entry) { clearInterval(poll); clearTimeout(timer); resolve(entry[1]); }
    }, 50);
    server.once('exit', code => { clearInterval(poll); clearTimeout(timer); reject(new Error(`Server exited ${code}: ${log}`)); });
    server.once('error', error => { clearInterval(poll); clearTimeout(timer); reject(error); });
  });
  const executablePath = process.env.CHROMIUM_PATH || (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined);
  browser = await chromium.launch({ headless: true, executablePath, args: ['--no-sandbox'] });
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  const formula = page.getByRole('textbox', { name: 'Formula input', exact: true });
  const trigger = page.getByRole('button', { name: 'Insert function', exact: true });
  const dialog = page.getByRole('dialog', { name: 'Insert function', exact: true });
  const search = dialog.getByRole('textbox', { name: 'Search functions' });
  const category = dialog.getByRole('combobox', { name: 'Category' });
  const option = name => dialog.getByRole('option', { name, exact: true });
  const active = locator => locator.evaluate(el => document.activeElement === el);
  const waitValue = async expected => {
    await page.waitForFunction(x => document.querySelector('input[aria-label="Formula input"]')?.value === x, expected);
    assert.equal(await formula.inputValue(), expected);
  };
  const catalogueReady = page.waitForResponse(response => response.url().endsWith('/api/functions'));
  await page.goto(base); await catalogueReady;
  await page.getByRole('gridcell', { name: 'B1', exact: true }).click();
  let writes = 0;
  page.on('request', request => { if (request.url().endsWith('/api/cells/set')) writes++; });

  await trigger.focus();
  await trigger.press('Enter');
  await option('ABS').waitFor();
  assert(await active(search), 'browser opens on Search');
  await category.selectOption('SQL');
  await option('SQLTABLE').waitFor();
  assert.equal(await option('SUM').count(), 0);
  await search.fill('tabular query');
  assert.equal(await option('SQLTABLE').getAttribute('aria-selected'), 'true');
  await dialog.getByText('=SQLTABLE(connection, query, [parameter, …])', { exact: true }).waitFor();
  await dialog.getByText('A value to bind to a SQL placeholder, in query order.').waitFor();
  await dialog.getByText('A spilling table of query results.').waitFor();
  await dialog.getByText('=SQLTABLE("LocalDB", "SELECT id, amount FROM invoices")', { exact: true }).waitFor();
  for (let i = 0; i < 7; i++) {
    await page.keyboard.press('Tab');
    assert.notEqual(await active(formula), true, 'Tab must not reach the worksheet while modal');
    assert.notEqual(await active(trigger), true, 'Tab must not reach the trigger while modal');
  }
  await page.keyboard.press('Escape');
  await dialog.waitFor({ state: 'hidden' });
  assert(await active(trigger), 'Escape restores trigger focus');

  await formula.fill('=IF(A1>0,SUX(A1:A3),0)');
  await formula.evaluate(el => el.setSelectionRange(11, 11));
  await trigger.click();
  await search.fill('sum');
  await option('SUM').waitFor();
  assert.equal(await option('SUM').getAttribute('aria-selected'), 'true');
  await search.press('ArrowDown');
  assert.equal(await option('SUMIF').getAttribute('aria-selected'), 'true');
  await search.press('ArrowUp');
  await search.press('Enter');
  await waitValue('=IF(A1>0,SUM(A1:A3),0)');
  assert(await active(formula), 'insertion restores formula editing focus');
  assert.equal(await formula.evaluate(el => el.selectionStart), 13);
  assert.equal(writes, 0, 'inserting a function does not commit the cell');

  await formula.fill('=A10');
  await trigger.click();
  await search.fill('round');
  await search.press('Enter');
  assert.equal(await dialog.count(), 1, 'Enter in Search does not overwrite an existing draft');
  await dialog.getByRole('button', { name: 'Replace draft', exact: true }).waitFor();
  await page.keyboard.press('Escape');
  await waitValue('=A10');
  assert.equal(writes, 0);
  assert.deepEqual(errors, []);
  console.log('PASS function browser details, filtering, keyboard and modal focus, nested insertion, and draft safety.');
} finally {
  if (browser) await browser.close();
  if (server.exitCode === null && server.signalCode === null && !server.killed) {
    const exited = new Promise(resolve => server.once('exit', resolve));
    server.kill('SIGTERM');
    await exited;
  }
  rmSync(fixture, { recursive: true, force: true });
}
