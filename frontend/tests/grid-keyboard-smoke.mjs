// Use the real server and Chromium with a synthetic workbook.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const fixture=mkdtempSync(path.join(os.tmpdir(),'gridloom-keyboard-'));
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0'],{cwd:root,env:{...process.env,GRIDLOOM_RECOVERY_DIR:path.join(fixture,'recovery')},stdio:['ignore','pipe','pipe']});
let output='',browser;
try {
  const base=await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(new Error(`Server timeout: ${output}`)),30_000);
    const read=chunk=>{output+=chunk;const match=output.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(match){clearTimeout(timer);resolve(match[1]);}};
    server.stdout.on('data',read);server.stderr.on('data',read);
    server.once('exit',code=>{clearTimeout(timer);reject(new Error(`Server exited ${code}: ${output}`));});
  });
  browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH,args:['--no-sandbox']});
  const page=await browser.newPage({viewport:{width:1280,height:850}});
  for(const [row,col,raw_input] of [[0,0,'1'],[1,0,'2'],[2,0,'=""'],[70,0,'4'],[70,1,'5'],[70,2,'6']]) {
    const response=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:'Sheet1',row,col,raw_input}});
    assert(response.ok(),await response.text());
  }
  await page.goto(base);
  const grid=page.getByRole('grid');
  const cell=name=>grid.getByRole('gridcell',{name,exact:true});
  const selected=name=>cell(name).and(grid.locator('[aria-selected="true"]')).waitFor();
  await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);
  await cell('A1').click();
  await grid.press('Control+ArrowDown');await selected('A3');
  await grid.press('Control+ArrowDown');await selected('A71'); // beyond the initial viewport
  await grid.press('Control+Shift+ArrowRight');await selected('A71');await selected('C71');
  await grid.press('Control+Space');await selected('C72');
  assert.equal(await cell('B71').getAttribute('aria-selected'),'false');
  await grid.press('Shift+Space');await selected('A71');await selected('C71');
  assert.equal(await cell('C72').getAttribute('aria-selected'),'false');
  await grid.press('Control+ArrowLeft');await selected('A71');
  await grid.press('Control+ArrowUp');await selected('A3');
  await cell('A3').dblclick();
  const editor=page.getByRole('textbox',{name:'Edit A3'});
  await editor.press('Control+ArrowDown');
  await editor.press('Shift+Space');
  await editor.waitFor();
  assert.equal(await editor.evaluate(el=>document.activeElement===el),true);
  await editor.press('Escape');
  await selected('A3');
  let jumps=0;
  page.on('request',request=>{if(request.url().includes('/api/cells/jump?')) jumps++;});
  await grid.evaluate(el=>el.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',ctrlKey:true,isComposing:true,bubbles:true,cancelable:true})));
  assert.equal(jumps,0);
  // A stale jump must not overwrite a newer ordinary move.
  let release,started;
  const hold=new Promise(resolve=>release=resolve), seen=new Promise(resolve=>started=resolve);
  await page.route('**/api/cells/jump?**',async route=>{
    started();await hold;
    await route.fulfill({status:200,json:{row:70,col:0}}).catch(()=>{});
  });
  await grid.press('Control+ArrowDown');await seen;
  await grid.press('ArrowRight');await selected('B3');
  release();
  await page.unrouteAll({behavior:'wait'});
  assert.equal(await cell('B3').getAttribute('aria-selected'),'true');
  // Huge but sparse fixture: exercise the normal Grid without populating a data table.
  const file=path.join(fixture,'deep.gridloom');
  const post=async(endpoint,data)=>{
    const response=await page.request.post(`${base}${endpoint}`,{data});
    assert(response.ok(),await response.text());
  };
  await post('/api/workbook/save',{path:file});
  execFileSync('python3', ['-c', [
    'import json, sqlite3, sys',
    'with sqlite3.connect(sys.argv[1]) as db:',
    '    saved = json.loads(db.execute("SELECT json FROM workbook WHERE singleton = 1").fetchone()[0])',
    '    saved["sheets"]["Sheet1"].update(max_rows=10_000_001, frozen_rows=2)',
    '    db.execute("UPDATE workbook SET json = ? WHERE singleton = 1", (json.dumps(saved),))',
  ].join('\n'), file]);
  await post('/api/workbook/open',{path:file});
  await post('/api/cells/set',{sheet_name:'Sheet1',row:10_000_000,col:0,raw_input:'=A1+41'});
  await page.reload();
  await cell('A1').getByText('1',{exact:true}).waitFor();
  const address=page.getByRole('textbox',{name:'Go to cell',exact:true});
  await address.fill('A71');await address.press('Enter');
  await selected('A71');
  await grid.focus();
  await grid.press('Control+ArrowDown');
  await selected('A10000001');
  await cell('A10000001').getByText('42',{exact:true}).waitFor();
  const positions=await grid.evaluate(el=>{
    const rect=name=>el.querySelector(`[role="gridcell"][aria-label="${name}"]`).getBoundingClientRect();
    const bounds=el.getBoundingClientRect();
    return {height:el.scrollHeight,pinnedTop:rect('A1').top-bounds.top,deepTop:rect('A10000001').top-bounds.top,deepBottom:rect('A10000001').bottom-bounds.top,visibleHeight:el.clientHeight};
  });
  assert(positions.height<=1_000_000);
  assert(positions.pinnedTop>=25 && positions.pinnedTop<=27);
  assert(positions.deepTop>=77, 'The selection is not covered by the frozen rows');
  assert(positions.deepBottom<=positions.visibleHeight+1, 'The selection is physically on screen');
  await grid.press('ArrowUp');await selected('A10000000');
  await grid.press('ArrowDown');await selected('A10000001');
  await grid.press('F2');
  const deepEditor=page.getByRole('textbox',{name:'Edit A10000001',exact:true});
  assert.equal(await deepEditor.inputValue(),'=A1+41');
  await deepEditor.fill('=A1+98');await deepEditor.press('Enter');
  await cell('A10000001').getByText('99',{exact:true}).waitFor();
  await page.waitForFunction(()=>document.querySelector('[role=grid]')?.getAttribute('aria-busy')==='false');
  await grid.press('Control+Space');
  await selected('A1');await selected('A10000001');
  assert.equal(await cell('B10000001').getAttribute('aria-selected'),'false');
  await grid.press('Control+Home');await selected('A1');
  console.log('PASS: worksheet keyboard navigation, Ctrl+Down final row reveal and editing, frozen top rows, and bounded segmented canvas.');
} finally {
  if(browser)await browser.close();
  if(server.exitCode===null && server.signalCode===null) {
    const stopped=new Promise(resolve=>server.once('exit',resolve));
    server.kill('SIGTERM');await stopped;
  }
  rmSync(fixture,{recursive:true,force:true});
}
