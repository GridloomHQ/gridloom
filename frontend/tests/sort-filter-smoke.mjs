// Test the read-only worksheet view against a real browser and a local server.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const recovery=mkdtempSync(path.join(tmpdir(),'gridloom-sort-smoke-'));
const server=spawn(process.env.GRIDLOOM_SERVER??path.join(root,'target/debug/gridloom-server'),['--port','0'],{cwd:root,env:{...process.env,GRIDLOOM_RECOVERY_DIR:recovery},stdio:['ignore','pipe','pipe']});
let log='',browser;
server.stdout.on('data',d=>log+=d);
server.stderr.on('data',d=>log+=d);
try {
  let base;
  const deadline=Date.now()+30_000;
  while(Date.now()<deadline&&server.exitCode===null&&!(base=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/)?.[1])) {
    await new Promise(resolve=>setTimeout(resolve,50));
  }
  assert(base,log);
  const executablePath=process.env.CHROMIUM_PATH ?? (!existsSync(chromium.executablePath())&&existsSync('/usr/bin/chromium')?'/usr/bin/chromium':undefined);
  browser=await chromium.launch({headless:true,executablePath});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  const post=async (route,data)=>{
    const response=await page.request.post(base+route,{data});
    assert(response.ok(),await response.text());
    return response.json();
  };
  const sheet_name='Sheet1';
  await post('/api/ranges/edit',{sheet_name,row:0,col:0,values:[
    ['Account','Amount'],
    ['Keep','9007199254740993.01'],
    ['Ignore','2'],
    ['KEEP','9007199254740993.00'],
    ['keep','9007199254740993.010']
  ]});
  await post('/api/cells/set',{sheet_name,row:0,col:3,raw_input:'=B2-B4'});
  const data={sheet_name,range:{start_row:0,start_col:0,end_row:4,end_col:3}};
  const before=await post('/api/ranges/capture',data);

  await page.goto(base);
  const cell=name=>page.getByRole('gridcell',{name,exact:true});
  await cell('A1').click();
  await cell('B5').click({modifiers:['Shift']});
  await page.getByRole('tab',{name:'Data',exact:true}).click();
  const trigger=page.getByRole('button',{name:'Sort & filter',exact:true});
  await trigger.click();
  const dialog=page.getByRole('dialog',{name:'Sort & filter',exact:true});
  await dialog.getByText('5 of 5 data rows',{exact:true}).waitFor();
  await dialog.getByLabel('First row is headers').check();
  await dialog.getByLabel('Sort by').selectOption('1');
  await dialog.getByLabel('Filter by').selectOption('0');
  await dialog.getByLabel('Condition').selectOption('equals');
  await dialog.getByLabel('Filter text').fill('keep');
  await dialog.getByRole('button',{name:'Apply view',exact:true}).click();
  await dialog.getByText('3 of 4 data rows',{exact:true}).waitFor();
  assert.deepEqual(await dialog.getByRole('button',{name:/Go to original row/}).allTextContents(),['4','2','5']);
  await dialog.getByRole('button',{name:'Go to original row 4',exact:true}).click();
  await dialog.waitFor({state:'hidden'});
  await page.waitForFunction(()=>document.querySelector('[role="gridcell"][aria-label="A4"]')?.getAttribute('aria-selected')==='true');
  assert.deepEqual(await post('/api/ranges/capture',data),before,'Sort and filter must not write or relocate worksheet cells');
  await trigger.click();
  await dialog.getByText('1 of 1 data rows',{exact:true}).waitFor();
  await post('/api/cells/set',{sheet_name,row:3,col:0,raw_input:'Changed after reading'});
  await dialog.getByRole('button',{name:'Go to original row 4',exact:true}).click();
  await dialog.getByRole('alert').filter({hasText:'Worksheet has changed.'}).waitFor();
  await dialog.getByRole('button',{name:'Apply view',exact:true}).click();
  await dialog.getByText('Changed after reading',{exact:true}).waitFor();
  await page.keyboard.press('Escape');
  await dialog.waitFor({state:'hidden'});
  console.log('Worksheet sort/filter browser regression passed');
} finally {
  await browser?.close();
  server.kill();
  rmSync(recovery,{recursive:true,force:true});
}
