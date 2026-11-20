import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const fixture = mkdtempSync(path.join(os.tmpdir(), 'gridloom-query-preview-'));
const server = spawn(process.env.GRIDLOOM_SERVER || path.join(root, 'target/debug/gridloom-server'),
  ['--port', '0', '--no-samples'], {
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
  const sql = 'SELECT id FROM items WHERE id >= 1 ORDER BY id DESC';
  const otherSql = 'SELECT id FROM other_items ORDER BY id';
  const connections = ['Ledger', 'Archive'].map(name => ({name, kind: 'sqlite', path: ':memory:'}));
  const columns = [{name: 'id', data_type: 'Integer', is_primary_key: true}];
  const emptyLineage = {is_editable: false, primary_keys: [], editable_columns: [], read_only_columns: [], read_only_reasons: []};
  let analysisCount = 0, executeCount = 0, pending, saved, inserted;
  let nextPageError = false;
  const requests = [];

  await page.route('**/api/connections', route => route.fulfill({json: connections}));
  await page.route('**/api/query/analyze', route => {
    analysisCount++;
    return route.fulfill({json: emptyLineage});
  });
  await page.route('**/api/query/execute', route => {
    executeCount++;
    return route.abort();
  });
  await page.route('**/api/sources', route => {
    if (route.request().method() !== 'POST') return route.continue();
    saved = route.request().postDataJSON();
    return route.fulfill({json: saved});
  });
  await page.route('**/api/tables/insert', route => {
    inserted = route.request().postDataJSON();
    return route.fulfill({json: {}});
  });
  await page.route('**/api/query/preview', async route => {
    const req = route.request().postDataJSON();
    requests.push(req);
    if (pending) {
      const blocked = pending;
      pending = undefined;
      blocked.started();
      await blocked.released;
    }
    if (req.offset === 100 && nextPageError) {
      nextPageError = false;
      return route.fulfill({status: 500, body: 'Fixture: page temporarily unavailable'});
    }
    const total = req.sql === sql ? 225 : 0;
    const count = Math.max(0, Math.min(req.limit, total - req.offset));
    await route.fulfill({json: {
      columns, offset: req.offset, has_more: total > req.offset + count,
      rows: Array.from({length: count}, (_, i) => [{type: 'Integer', value: total - req.offset - i}]),
    }}).catch(() => {}); // A stale request may have been aborted by an editor change.
  });
  const waitForBlockedPage = () => {
    let start, release;
    const started = new Promise(resolve => { start = resolve; });
    pending = {started: start, released: new Promise(resolve => { release = resolve; })};
    return {started, release};
  };
  await page.goto(base);
  const open = page.getByRole('button', {name: 'SQL Query', exact: true});
  await page.getByRole('tab', {name: 'Data', exact: true}).click();
  await open.click();
  const dialog = page.getByRole('dialog', {name: 'SQL Query Editor & Source Builder', exact: true});
  const editor = dialog.getByRole('textbox', {name: 'SQL query', exact: true});
  const conn = dialog.getByRole('combobox', {name: 'Target Database:'});
  const run = dialog.getByRole('button', {name: 'Run Query', exact: true});
  const next = dialog.getByRole('button', {name: 'Next', exact: true});
  const prev = dialog.getByRole('button', {name: 'Previous', exact: true});
  const save = dialog.getByRole('button', {name: 'Save Source & Insert Table', exact: true});
  const waitStatus = name => dialog.getByRole('status').filter({hasText: name}).waitFor();

  await editor.fill(sql);
  await run.click();
  await waitStatus('Rows 1–100');
  assert(await prev.isDisabled());
  assert.deepEqual(requests[0], {connection_name: 'Ledger', sql, offset: 0, limit: 100});
  assert.equal(await dialog.locator('tbody tr').count(), 100);
  assert.equal(await dialog.locator('tbody tr').first().innerText(), '225');
  nextPageError = true;
  await next.click();
  await dialog.getByRole('alert').getByText('Fixture: page temporarily unavailable').waitFor();
  await waitStatus('Rows 1–100');
  await next.click();
  await waitStatus('Rows 101–200');
  await next.click();
  await waitStatus('Rows 201–225');
  assert(await next.isDisabled());
  assert.equal(await dialog.locator('tbody tr').count(), 25);
  await prev.click();
  await waitStatus('Rows 101–200');
  assert.equal(analysisCount, 1);
  assert.deepEqual(requests.map(r => r.offset), [0, 100, 100, 200, 100]);
  assert(requests.every(r => r.sql === sql && r.limit === 100));

  const pendingSql = waitForBlockedPage();
  await next.click();
  await pendingSql.started;
  await editor.fill(otherSql);
  assert(await save.isDisabled());
  assert.equal(await next.count(), 0);
  pendingSql.release();
  await run.click();
  await waitStatus('No matching rows');
  assert(await next.isDisabled());
  assert.equal(requests.at(-1).offset, 0);
  assert.equal(requests.at(-1).sql, otherSql);

  await editor.fill(sql);
  await run.click();
  await waitStatus('Rows 1–100');
  const pendingConn = waitForBlockedPage();
  await next.click();
  await pendingConn.started;
  await conn.selectOption('Archive');
  assert(await save.isDisabled());
  assert.equal(await next.count(), 0);
  pendingConn.release();
  await run.click();
  await waitStatus('Rows 1–100');
  assert.deepEqual(requests.at(-1), {connection_name: 'Archive', sql, offset: 0, limit: 100});

  await next.click();
  await waitStatus('Rows 101–200');
  await dialog.getByRole('textbox', {name: 'Source Name:'}).fill('Saved test query');
  const insertion = page.waitForResponse(response => response.url().endsWith('/api/tables/insert'));
  await save.click();
  await insertion;
  await dialog.waitFor({state: 'hidden'});
  assert.deepEqual(saved, {name: 'Saved test query', connection_name: 'Archive', sql});
  assert.equal(inserted?.source_name, 'Saved test query');
  assert.equal(executeCount, 0);
  assert.deepEqual(errors, []);
  console.log('PASS SQL pagination, failed-page retry, stale query/connection changes, untouched source SQL.');
} finally {
  if (browser) await browser.close();
  if (server.exitCode === null && server.signalCode === null && !server.killed) {
    const exited = new Promise(resolve => server.once('exit', resolve));
    server.kill('SIGTERM');
    await exited;
  }
  rmSync(fixture, { recursive: true, force: true });
}
