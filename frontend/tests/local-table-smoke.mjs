import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const fixture=mkdtempSync(path.join(os.tmpdir(),'gridloom-local-table-'));
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0'],{
  cwd:root, env:{...process.env,GRIDLOOM_RECOVERY_DIR:path.join(fixture,'recovery')},stdio:['ignore','pipe','pipe'],
});
let browser;
try {
  const base=await new Promise((resolve,reject)=>{
    const timeout=setTimeout(()=>reject(new Error('Gridloom did not start')),10000);
    server.stdout.on('data',chunk=>{
      const match=String(chunk).match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);
      if(match){clearTimeout(timeout);resolve(match[1]);}
    });
    server.once('error',error=>{clearTimeout(timeout);reject(error)});
    server.once('exit',code=>{clearTimeout(timeout);reject(new Error('Gridloom exited: '+code))});
  });
  browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH,args:['--no-sandbox']});
  const page=await browser.newPage();
  const post=async(endpoint,data)=>{
    const response=await page.request.post(base+endpoint,{data});
    assert(response.ok(),await response.text());
    return response.json();
  };
  await post('/api/sheets',{name:'Budget'});
  await post('/api/sheets/select',{name:'Budget'});
  await post('/api/ranges/edit',{sheet_name:'Budget',row:0,col:0,values:[['Account','Amount'],['0007','0.1']]});
  await page.goto(base);
  const cell=name=>page.getByRole('gridcell',{name,exact:true});
  const grid=page.getByRole('grid',{name:/worksheet/i});
  await cell('A1').getByText('Account',{exact:true}).waitFor();
  await cell('A1').click();
  await grid.press('Shift+ArrowRight');
  await grid.press('Shift+ArrowDown');
  await grid.press('Shift+ArrowDown');
  await page.getByRole('tab',{name:'Insert',exact:true}).click();
  await page.getByRole('button',{name:'Create local table',exact:true}).click();
  const dialog=page.getByRole('dialog',{name:'Create local table',exact:true});
  await dialog.getByLabel('Table name').fill('Transactions');
  await dialog.getByLabel('Column A type').selectOption('Text');
  await dialog.getByLabel('Column B type').selectOption('Decimal');
  await dialog.getByRole('button',{name:'Create table',exact:true}).click();
  await page.getByText('Created local table Transactions',{exact:true}).waitFor();
  await cell('A2').getByText('0007',{exact:true}).waitFor();

  await cell('B3').click();
  await grid.press('F2');
  const editor=page.getByRole('textbox',{name:'Edit B3',exact:true});
  await editor.fill('0.2');
  await editor.press('Enter');
  await cell('B3').getByText('0.2',{exact:true}).waitFor();
  await post('/api/cells/set',{sheet_name:'Budget',row:4,col:0,raw_input:'=SUM(Transactions[Amount])'});
  await page.reload();
  await cell('A5').getByText('0.3',{exact:true}).waitFor();

  const file=path.join(fixture,'budget.gridloom');
  await post('/api/workbook/save',{path:file});
  const opened=await page.request.get(base+'/api/workbook');
  const summary=await opened.json();
  const table=summary.sheets.find(sheet=>sheet.name==='Budget').tables[0];
  assert.equal(table.name,'Transactions');
  assert.deepEqual(table.local.column_types,['Text','Decimal']);
  assert.deepEqual(summary.connections,[]);
  assert.equal(summary.staged_patches_count,0);
  assert(readFileSync(file).subarray(0,16).equals(Buffer.from('SQLite format 3\0')));
  await post('/api/workbook/open',{path:file});
  await page.reload();
  await cell('A2').getByText('0007',{exact:true}).waitFor();
  await cell('A5').getByText('0.3',{exact:true}).waitFor();
  console.log('Verified local table creation, typed edit, structured formula, exact SQLite persistence and reopen in Chromium.');
} finally {
  await browser?.close();
  server.kill();
  rmSync(fixture,{recursive:true,force:true});
}
