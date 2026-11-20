// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/selection-summary-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const seed=async(sheet,row,col,raw_input)=>{const response=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:sheet,row,col,raw_input}});assert.equal(response.ok(),true,await response.text());};
 await seed('Sheet1',0,0,'0.1');await seed('Sheet1',1,0,'0.2');await seed('Sheet1',2,0,'not a number');
 await seed('Sheet1',0,1,'9007199254740993');await seed('Sheet1',1,1,'2');
 await seed('Sheet1',0,2,'1e-10');await seed('Sheet1',0,3,'=1/0');await seed('Sheet1',0,4,'1e100');
 const fixtureResponse=await page.request.post(`${base}/api/cells/viewport`,{data:{sheet_name:'Sheet1',start_row:0,start_col:0,end_row:0,end_col:4,overscan:0}});assert.equal(fixtureResponse.ok(),true);const fixtureViewport=await fixtureResponse.json();assert.equal(fixtureViewport.cells.C1.raw_value.type,'Float','Approximation fixture must remain an actual Float, not an exact Decimal');assert.equal(fixtureViewport.cells.E1.raw_value.type,'Float');
 await page.goto(base);const grid=()=>page.getByRole('grid');const cell=name=>page.getByRole('gridcell',{name,exact:true});
 const address=page.getByRole('textbox',{name:'Go to cell',exact:true});
 const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const go=async name=>{await address.focus();await idle();await address.fill(name);await address.press('Enter');await page.waitForFunction(name=>document.querySelector('input[aria-label="Go to cell"]')?.value===name,name);};
 const range=async(first,last)=>{await go(first);await cell(last).click({modifiers:['Shift']});};
 await grid().waitFor();
 const summary=page.getByRole('region',{name:'Selection summary',exact:true});
 const sum=summary.getByLabel('Selection sum',{exact:true});
 const avg=summary.getByLabel('Selection average',{exact:true});
 const numbers=summary.getByLabel('Selection numeric count',{exact:true});
 const expectMetric=async(locator,text)=>{await locator.filter({hasText:text}).waitFor();assert.equal(await locator.innerText(),text);};
 await range('A1','A2');await expectMetric(sum,'0.3');await expectMetric(avg,'0.15');await expectMetric(numbers,'2');
 await range('A1','A3');await expectMetric(sum,'0.3');await expectMetric(avg,'0.15');await expectMetric(numbers,'2');await summary.getByText('Selected: 3',{exact:true}).waitFor();
 await range('B1','B2');await expectMetric(sum,'9007199254740995');await expectMetric(avg,'4503599627370497.5');await expectMetric(numbers,'2');
 await go('C1');await sum.filter({hasText:'(approximate)'}).waitFor();assert.match(await avg.innerText(),/\(approximate\)$/);await expectMetric(numbers,'1');
 await go('E1');await expectMetric(sum,'—');await expectMetric(avg,'—');await expectMetric(numbers,'1');
 await go('D1');await expectMetric(sum,'—');await expectMetric(avg,'—');await expectMetric(numbers,'0');assert.match(await summary.getAttribute('title'),/errors 1/);
 // Hold a real response, allow a newer context to finish, then release the stale
 // result. This exercises real computations rather than a synthetic result shape.
 const staleSummary=async(action,verify)=>{
   let held=false;const started=deferred(),resume=deferred(),done=deferred();
   const handler=async route=>{
     if(held){await route.continue();return;}
     held=true;const response=await route.fetch();started.resolve();await resume.promise;await route.fulfill({response});done.resolve();
   };
   await page.route('**/api/ranges/summary',handler);await go('A2');await started.promise;
   await action();await verify();const current=await summary.innerText();
   const staleResponse=page.waitForResponse(response=>response.url().endsWith('/api/ranges/summary'));
   resume.resolve();await done.promise;await staleResponse;await page.unroute('**/api/ranges/summary',handler);
   await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
   assert.equal(await summary.innerText(),current,'Stale summary must not replace the newer selection context');
 };
 await staleSummary(()=>go('B1'),()=>expectMetric(sum,'9007199254740993'));
 await staleSummary(async()=>{page.once('dialog',d=>d.accept('Empty summary sheet'));await page.getByRole('button',{name:'Add worksheet',exact:true}).click();await page.getByRole('grid',{name:'Empty summary sheet worksheet'}).waitFor();},()=>expectMetric(numbers,'0'));
 await page.getByRole('button',{name:'Sheet1',exact:true}).click();await page.getByRole('grid',{name:'Sheet1 worksheet'}).waitFor();await idle();await go('B1');await expectMetric(sum,'9007199254740993');
 await staleSummary(async()=>{await page.getByRole('tab',{name:'File',exact:true}).click();await page.getByRole('button',{name:'New',exact:true}).click();await page.getByRole('button',{name:'Discard and continue',exact:true}).click();await page.getByRole('grid',{name:'Sheet1 worksheet'}).waitFor();await idle();},()=>expectMetric(numbers,'0'));
 // Selection bounds alone exercise the cap; no huge seed or backend request needed.
 let summaryRequests=0;const countRequest=request=>{if(request.url().endsWith('/api/ranges/summary'))summaryRequests++;};
 await go('K1000');await grid().focus();await grid().press('Control+Shift+Home');await summary.getByText('Summary unavailable: select at most 10,000 cells (11000 selected).',{exact:true}).waitFor();
 page.on('request',countRequest);await page.waitForTimeout(300);page.off('request',countRequest);assert.equal(summaryRequests,0,'Oversized selection must not request a backend summary');
 await go('A1');await expectMetric(numbers,'0');
 await page.screenshot({path:path.join(artifacts,'selection-summary.png'),fullPage:true});assert.deepEqual(errors,[]);
 console.log('PASS selection summary: exact decimals and large integers, exclusions, approximate/error labels, stale selection/sheet/workbook responses, 10,000-cell limit and recovery.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
