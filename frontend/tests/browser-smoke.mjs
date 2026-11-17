// Real browser + real local server, synthetic data only. Build both before running.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/browser-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='';server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
let browser;
try{
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(new Error(`Server timeout: ${log}`)),30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.on('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 const context=await browser.newContext({viewport:{width:1440,height:1000},permissions:['clipboard-read','clipboard-write']});
 const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(base);await page.getByRole('grid',{name:'Sheet1 worksheet'}).waitFor();
 const cell=name=>page.getByRole('gridcell',{name,exact:true});
 async function edit(name,value){await cell(name).dblclick();const input=page.getByRole('textbox',{name:`Edit ${name}`,exact:true});await input.fill(value);await input.press('Enter');await page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');}
 async function expectText(locator,value){await locator.filter({hasText:value}).waitFor();}
 await edit('A1','2');await edit('B1','=A1*3');await expectText(cell('B1'),'6');
 await cell('B1').click();await cell('B3').click({modifiers:['Shift']});await page.getByRole('button',{name:'Fill down',exact:true}).click();await expectText(cell('B2'),'0');
 await page.getByRole('button',{name:'Undo',exact:true}).click();await page.waitForFunction(()=>document.querySelector('[role="gridcell"][aria-label="B2"]')?.textContent==='');
 await page.getByRole('button',{name:'Redo',exact:true}).click();await expectText(cell('B2'),'0');
 await cell('A1').click();await page.getByTitle('Format as Currency ($)',{exact:true}).click();await expectText(cell('A1'),'$ 2.00');
 await page.getByTitle('Toggle bold',{exact:true}).click();await page.waitForFunction(()=>getComputedStyle(document.querySelector('[role="gridcell"][aria-label="A1"]')).fontWeight==='700');
 await cell('A1').click();await cell('B1').click({modifiers:['Shift']});await page.waitForTimeout(200);await page.keyboard.press('Control+c');await cell('D1').click();await page.keyboard.press('Control+v');await expectText(cell('E1'),'6');
 page.once('dialog',d=>d.accept('Second sheet'));await page.getByRole('button',{name:'Add worksheet',exact:true}).click();await page.getByRole('grid',{name:'Second sheet worksheet'}).waitFor();
 await edit('A1','=Sheet1!B1+1');await expectText(cell('A1'),'7');
 await page.getByRole('button',{name:'Monthly close',exact:true}).click();const modal=page.getByRole('dialog',{name:'Monthly close',exact:true});
 await modal.getByLabel('Import period',{exact:true}).fill('2026-10');
 const csv='account,name,debit,credit\n1000,Cash,10000,0\n1200,Processor clearing,5000,0\n4000,Sales,0,16000\n5000,Processor fees,1000,0';
 await modal.getByLabel('Trial balance CSV',{exact:true}).fill(csv);await modal.getByRole('button',{name:'Import and generate P&L',exact:true}).dblclick();
 assert.equal(await modal.getByLabel('Existing close period',{exact:true}).locator('option').count(),2,'Repeated import clicks must create exactly one period');
 await modal.getByLabel('Mapping 1000',{exact:true}).selectOption('Balance sheet');await modal.getByLabel('Mapping 1200',{exact:true}).selectOption('Balance sheet');await modal.getByLabel('Mapping 4000',{exact:true}).selectOption('Revenue');await modal.getByLabel('Mapping 5000',{exact:true}).selectOption('Cost of sales');
 await modal.getByRole('button',{name:'Save account mappings',exact:true}).click();await modal.getByText('Mappings saved and P&L recalculated.',{exact:true}).waitFor();await modal.getByText('Trial balance balances',{exact:true}).waitFor();
 await modal.getByRole('button').filter({hasText:'Net profit'}).click();
 const panel=page.getByRole('complementary',{name:'Explain and review selected cell'});await panel.getByText('Current calculated value',{exact:true}).waitFor();
 await panel.getByLabel('Reviewer name or initials',{exact:true}).fill('Synthetic reviewer');await panel.getByLabel('Review note',{exact:true}).fill('Settlement tied to dated 2026-10-31 source snapshot; materiality threshold 100. Local synthetic acceptance case.');
 await panel.getByLabel('Attach support files',{exact:true}).setInputFiles({name:'settlement-2026-10-31.txt',mimeType:'text/plain',buffer:Buffer.from('Dated synthetic processor settlement: gross 16000; fees 1000; net profit 15000. Materiality 100.')});
 await panel.getByText(/settlement-2026-10-31.txt · \d+ bytes/).waitFor();
 await panel.getByRole('button',{name:'Record review',exact:true}).dblclick();await panel.getByText('Review recorded in this workbook. Save the workbook to keep it on disk.',{exact:true}).waitFor();
 await panel.getByRole('button',{name:'Download settlement-2026-10-31.txt · v1',exact:true}).first().waitFor();
 await page.getByRole('button',{name:'Monthly close',exact:true}).click();await modal.getByLabel('Existing close period',{exact:true}).selectOption({label:'2026-10 · version 1 · Close 2026-10 v1'});await modal.getByLabel('Mapping 4000',{exact:true}).selectOption('Other income');await modal.getByRole('button',{name:'Save account mappings',exact:true}).click();await modal.getByText('Mappings saved and P&L recalculated.',{exact:true}).waitFor();
 await modal.getByRole('button').filter({hasText:'Net profit'}).filter({hasText:'Needs re-review'}).click();await panel.getByText('Needs re-review',{exact:true}).first().waitFor();await panel.getByRole('button',{name:'Compare exact changes',exact:true}).click();await panel.getByText(/captured input changes/).waitFor();
 await page.screenshot({path:path.join(artifacts,'stale-review-comparison.png'),fullPage:true});
 await page.getByRole('button',{name:'Monthly close',exact:true}).click();await modal.getByLabel('Existing close period',{exact:true}).selectOption({label:'2026-10 · version 1 · Close 2026-10 v1'});await modal.getByRole('button',{name:'Export review pack',exact:true}).waitFor();
 const downloadEvent=page.waitForEvent('download');await modal.getByRole('button',{name:'Export review pack',exact:true}).click();const download=await downloadEvent;const downloadPath=await download.path();const pack=JSON.parse(readFileSync(downloadPath,'utf8'));assert.equal(pack.format,'gridloom-review-pack');assert.equal(pack.status.period.source_csv,csv);assert.equal(pack.reviews.length,1,'Duplicate import/review clicks must not duplicate state');assert.equal(pack.reviews[0].attachments[0].filename,'settlement-2026-10-31.txt');assert(pack.reviews[0].input_snapshot.inputs);
 await modal.getByText('Import trial balance / roll forward',{exact:true}).click();await modal.getByLabel('Import period',{exact:true}).fill('2026-11');await modal.getByLabel('Trial balance CSV',{exact:true}).fill(csv);await modal.getByRole('button',{name:'Roll forward selected period',exact:true}).click();await modal.getByText('Reconciliation · 2026-11',{exact:true}).waitFor();
 await page.screenshot({path:path.join(artifacts,'monthly-close-rollforward.png'),fullPage:true});
 await modal.getByRole('button',{name:'Close monthly close',exact:true}).click();
 const saved=path.join(artifacts,'acceptance.gridloom');page.once('dialog',d=>d.accept(saved));await page.getByRole('button',{name:'Save',exact:true}).click();await page.getByText(`Saved workbook to ${saved}`,{exact:true}).waitFor();
 page.once('dialog',d=>d.accept());await page.getByRole('button',{name:'New',exact:true}).click();await page.getByRole('grid',{name:'Sheet1 worksheet'}).waitFor();page.once('dialog',d=>d.accept(saved));await page.getByRole('button',{name:'Open',exact:true}).click();await page.getByRole('grid',{name:'Close 2026-11 v1 worksheet'}).waitFor();
 assert.equal(errors.length,0,errors.join('\n'));console.log('PASS real browser: ranges/format/fill/undo/copy, cross-sheet formula, monthly import/map/reconcile, dated support/materiality note, stale same-total mapping review, exact compare/export, rollforward, save/reopen.');
}finally{if(browser)await browser.close();server.kill('SIGTERM');}
