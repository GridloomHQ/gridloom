// Use the real server and Chromium with a synthetic workbook.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,stdio:['ignore','pipe','pipe']});
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
  console.log('PASS: sparse jumps, whole-axis selection, edit/IME handling, and stale jump cancellation.');
} finally {
  if(browser)await browser.close();
  server.kill('SIGTERM');
}
