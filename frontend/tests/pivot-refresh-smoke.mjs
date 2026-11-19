import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const temp = mkdtempSync(path.join(os.tmpdir(), 'gridloom-pivot-smoke-'));
const db = path.join(temp, 'entries.sqlite');
const sql = statement => execFileSync('python3', ['-c',
  'import sqlite3,sys; db=sqlite3.connect(sys.argv[1]); db.executescript(sys.argv[2]); db.close()',
  db, statement,
]);
sql("CREATE TABLE entries(id INTEGER PRIMARY KEY, account TEXT, cents INTEGER); INSERT INTO entries VALUES(1,'A',10);");
const binary = process.env.GRIDLOOM_SERVER ?? path.join(root, 'target/debug/gridloom-server');
const server = spawn(binary, ['--port', '0', '--no-samples'], {
  cwd: root, env: {...process.env, GRIDLOOM_RECOVERY_DIR: path.join(temp, 'recovery')},
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
  const api = async (route, payload) => {
    const response = await fetch(base + '/api/' + route, {
      method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify(payload),
    });
    assert(response.ok, await response.text());
  };
  await api('connections', {name:'Test', kind:'sqlite', path:db});
  await api('sources', {name:'EntrySource', connection_name:'Test', sql:'SELECT id, account, cents FROM entries'});
  await api('pivots', {name:'Balances', source_table:'EntrySource', row_fields:['account'], col_fields:[], value_field:'cents', aggregation:'Sum'});

  browser = await chromium.launch({headless:true, executablePath:process.env.CHROMIUM_PATH});
  const page = await browser.newPage();
  await page.goto(base);
  await page.getByRole('button', {name:'Pivot', exact:true}).click();
  const modal = page.getByRole('dialog', {name:'Pivot Table Builder'});
  await modal.getByLabel('Saved pivot').selectOption('Balances');
  await modal.getByText('Balances · Workbook source snapshot', {exact:true}).waitFor();
  await modal.getByRole('cell', {name:'10', exact:true}).waitFor();
  sql("INSERT INTO entries VALUES(2,'A',5);");
  await modal.getByRole('button', {name:'Refresh from source', exact:true}).click();
  await modal.getByText('Balances · Source query rerun; workbook snapshot unchanged', {exact:true}).waitFor();
  await modal.getByRole('cell', {name:'15', exact:true}).waitFor();
  sql('DROP TABLE entries;');
  await modal.getByRole('button', {name:'Refresh from source', exact:true}).click();
  await modal.getByRole('alert').filter({hasText:'The previous result is still shown.'}).waitFor();
  assert.equal(await modal.getByRole('cell', {name:'15', exact:true}).count(), 1);
  console.log('Pivot refresh browser regression passed.');
} finally {
  await browser?.close();
  if (server.exitCode === null && server.signalCode === null) {
    const exited = new Promise(resolve => server.once('exit', resolve));
    if (server.kill()) await exited;
  }
  rmSync(temp, {recursive:true, force:true});
}
