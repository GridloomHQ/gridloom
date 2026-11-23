// Real UI smoke for the Tayriva-only clipboard. Use an isolated recovery directory.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const recovery = mkdtempSync(path.join(tmpdir(), 'gridloom-paste-smoke-'));
const server = spawn(process.env.GRIDLOOM_SERVER || path.join(root, 'target/debug/gridloom-server'),
  ['--port', '0'], {cwd:root, env:{...process.env, GRIDLOOM_RECOVERY_DIR:recovery}});
let log = '', browser;
server.stdout.on('data', d => log += d);
server.stderr.on('data', d => log += d);
try {
  const deadline = Date.now() + 30000;
  let base;
  while (Date.now() < deadline && server.exitCode === null &&
    !(base = log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/)?.[1])) {
    await new Promise(r => setTimeout(r, 50));
  }
  assert(base, log);
  const executablePath = process.env.CHROMIUM_PATH ??
    (existsSync(chromium.executablePath()) ? undefined : existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined);
  browser = await chromium.launch({headless:true, executablePath});
  const page = await browser.newPage({viewport:{width:1440,height:1000}});
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const post = async (route, data) => {
    const response = await page.request.post(base + '/api/' + route, {data});
    assert(response.ok(), await response.text());
    return response.json();
  };
  await post('ranges/edit', {sheet_name:'Sheet1',row:0,col:0,
    values:[['17','=9007199254740993.123456','=A1+$A$1']]});
  await page.goto(base);
  const grid = page.getByRole('grid');
  const cell = name => page.getByRole('gridcell', {name, exact:true});
  const menu = page.getByRole('group', {name:'Paste Special options'});
  const copy = async (addr, expected) => {
    const wait = page.waitForResponse(r => r.url().endsWith('/api/ranges/capture'));
    await cell(addr).click();
    await (await wait).finished();
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const result = await grid.evaluate(el => {
      const clipboardData = new DataTransfer();
      el.dispatchEvent(new ClipboardEvent('copy',{bubbles:true,clipboardData}));
      return clipboardData.getData('text/plain');
    });
    assert.equal(result, expected);
  };
  const read = async addr => {
    const view = await post('cells/viewport', {sheet_name:'Sheet1',start_row:0,start_col:0,end_row:4,end_col:4,overscan:0});
    return view.cells[addr];
  };
  await page.getByRole('button', {name:'Paste Special',exact:true}).click();
  assert.equal(await menu.getByRole('button', {name:'Values only'}).isDisabled(), true);
  await menu.getByRole('button',{name:'Close',exact:true}).click();
  await copy('B1', '=9007199254740993.123456');
  await cell('B2').click();
  await page.getByRole('tab',{name:'File',exact:true}).click();
  await grid.press('Control+Alt+v');
  await menu.getByText(/Last worksheet copy: Sheet1!B1/).waitFor();
  await menu.getByRole('button',{name:'Values only'}).click();
  await page.getByText('Pasted values only',{exact:true}).waitFor();
  const values = await read('B2');
  assert.deepEqual(values.raw_value,{type:'Decimal',value:'9007199254740993.123456'});
  assert.equal(values.formula,null);
  await page.getByRole('tab',{name:'Home',exact:true}).click();
  await copy('C1','=A1+$A$1');
  await cell('C2').click();
  await page.getByRole('button',{name:'Paste Special',exact:true}).click();
  await menu.getByRole('button',{name:'Formulas only'}).click();
  await page.getByText('Pasted formulas only',{exact:true}).waitFor();
  assert.equal((await read('C2')).formula,'=A2+$A$1');
  await page.getByRole('button',{name:'Keyboard shortcuts',exact:true}).click();
  const shortcuts = page.getByRole('dialog',{name:'Worksheet keyboard shortcuts'});
  await shortcuts.getByText('Ctrl+Alt+V').waitFor();
  assert.equal(await shortcuts.evaluate(el => el.matches(':modal')),true,'Shortcuts must use the top layer above worksheet headers');
  assert.equal(await shortcuts.getByRole('button',{name:'Close shortcuts'}).evaluate(el => el === document.activeElement),true);
  await page.keyboard.press('Escape');
  await shortcuts.waitFor({state:'hidden'});
  assert.equal(await page.getByRole('button',{name:'Keyboard shortcuts',exact:true}).evaluate(el => el === document.activeElement),true);
  // At phone width, the toolbar remains one row with two independently scrollable areas.
  await page.setViewportSize({width:390,height:740});
  const home = page.getByRole('tab',{name:'Home',exact:true});
  await home.focus();
  await page.keyboard.press('End');
  const help = page.getByRole('tab',{name:'Help',exact:true});
  assert.equal(await help.getAttribute('aria-selected'),'true');
  assert.equal(await help.evaluate(el => el === document.activeElement),true);
  await page.keyboard.press('ArrowLeft');
  const addons = page.getByRole('tab',{name:'Add-ons',exact:true});
  assert.equal(await addons.getAttribute('aria-selected'),'true');
  const addOnPanel = page.getByRole('tabpanel',{name:'Add-ons',exact:true});
  const [tabBox, controlBox] = await Promise.all([
    addons.boundingBox(), addOnPanel.getByRole('checkbox',{name:'Enable Monthly close add-on'}).boundingBox()
  ]);
  assert(tabBox && controlBox);
  assert(Math.abs(tabBox.y + tabBox.height/2 - controlBox.y - controlBox.height/2) < 3,'Tabs and commands must share one row');
  assert.equal(await page.getByRole('tabpanel').count(),1);
  await page.keyboard.press('Tab');
  assert.equal(await addOnPanel.getByRole('checkbox').evaluate(el => el === document.activeElement),true);
  assert.deepEqual(errors, []);
  console.log('PASS: Paste Special via keyboard and menu, exact decimals, adjusted formulas, and shortcut help');
} finally {
  await browser?.close();
  server.kill('SIGTERM');
  rmSync(recovery,{recursive:true,force:true});
}
