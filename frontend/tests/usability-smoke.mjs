// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/usability-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(base);const grid=()=>page.getByRole('grid');const cell=name=>page.getByRole('gridcell',{name,exact:true});const address=page.getByRole('textbox',{name:'Go to cell',exact:true});
 const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const selected=async name=>{await page.waitForFunction(name=>document.querySelector('input[aria-label="Go to cell"]')?.value===name,name);assert.equal(await cell(name).getAttribute('aria-selected'),'true');};
 const go=async name=>{await address.fill(name);await address.press('Enter');await selected(name);};
 const edit=async(name,value,key='Enter')=>{await cell(name).dblclick();const input=page.getByRole('textbox',{name:`Edit ${name}`,exact:true});await input.fill(value);await input.press(key);await idle();};
 await grid().waitFor();await edit('A1','12');await selected('A2');await grid().press('Shift+Enter');await selected('A1');
 await edit('A1','13','Tab');await selected('B1');await grid().press('Shift+Tab');await selected('A1');
 await grid().press('F2');await page.getByRole('textbox',{name:'Edit A1',exact:true}).fill('discard me');await page.keyboard.press('Escape');assert.equal(await cell('A1').innerText(),'13');await selected('A1');
 // Failed writes retain the draft and selection; retry writes once and moves only on success.
 let attempts=0;const failure=async route=>{attempts++;await route.fulfill({status:409,contentType:'application/json',body:JSON.stringify({error:'Synthetic write failure'})});};
 await page.route('**/api/cells/set',failure);await cell('A1').dblclick();let input=page.getByRole('textbox',{name:'Edit A1',exact:true});await input.fill('14');await input.press('Enter');await page.getByText(/Error:.*Synthetic write failure/).waitFor();await idle();assert.equal(await input.inputValue(),'14');await selected('A1');assert.equal(attempts,1);
 await page.unroute('**/api/cells/set',failure);await input.press('Enter');await selected('A2');await idle();assert.equal(await cell('A1').innerText(),'14');
 // A failed draft belongs to its original cell, never to a later Go to target.
 await page.route('**/api/cells/set',failure);await cell('A1').dblclick();input=page.getByRole('textbox',{name:'Edit A1',exact:true});await input.fill('must not reach B2');await input.press('Enter');await idle();await page.getByText(/Error:.*Synthetic write failure/).waitFor();
 assert.equal(await input.inputValue(),'must not reach B2');
 // Focusing the address box may retry the failed draft on blur. Let that failure
 // settle before navigating, and prove navigation itself cannot submit to B2.
 await address.fill('B2');await idle();const beforeNavigate=attempts;await address.press('Enter');await selected('B2');
 assert.equal(await page.getByRole('textbox',{name:'Edit B2',exact:true}).count(),0);
 assert.equal(await page.getByRole('textbox',{name:'Edit A1',exact:true}).count(),0);
 assert.equal(attempts,beforeNavigate);assert.equal(await cell('B2').innerText(),'');assert.equal(await cell('A1').innerText(),'14');
 await page.unroute('**/api/cells/set',failure);await grid().focus();await grid().press('Enter');await selected('B3');assert.equal(await cell('B2').innerText(),'');
 // Repeated Enter during a delayed mutation must not submit again or move twice.
 const received=deferred(),release=deferred();let submissions=0;
 const delay=async route=>{submissions++;received.resolve();await release.promise;await route.continue();};await page.route('**/api/cells/set',delay);
 await cell('A2').dblclick();input=page.getByRole('textbox',{name:'Edit A2',exact:true});await input.fill('15');await input.press('Enter');await received.promise;await input.press('Enter');assert.equal(await address.inputValue(),'A2');release.resolve();await selected('A3');await idle();assert.equal(submissions,1);await page.unroute('**/api/cells/set',delay);
 // Navigation is bounded and distant cells become visible; invalid addresses do not move.
 await go('D200');await grid().focus();await grid().press('Home');await selected('A200');await grid().press('Control+Home');await selected('A1');
 await grid().press('PageDown');const pageDown=await address.inputValue();assert.match(pageDown,/^A\d+$/);assert.notEqual(pageDown,'A1');await grid().press('PageUp');await selected('A1');
 await address.fill('A0');await address.press('Enter');await page.getByRole('alert').filter({hasText:'Enter a cell address'}).waitFor();assert.equal(await cell('A1').getAttribute('aria-selected'),'true');await address.press('Escape');assert.equal(await address.inputValue(),'A1');
 // Offscreen data must arrive before editing; a placeholder is not an empty cell.
 await go('D200');await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);await edit('D200','existing distant value');await go('A1');await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);
 const viewportStarted=deferred(),viewportResume=deferred();
 const delayedViewport=async route=>{
   const bounds=route.request().postDataJSON();
   if(bounds.start_row>100){viewportStarted.resolve();await viewportResume.promise;}
   await route.continue();
 };
 await page.route('**/api/cells/viewport',delayedViewport);await go('D200');await viewportStarted.promise;
 await grid().focus();await grid().press('F2');assert.equal(await page.getByRole('textbox',{name:'Edit D200',exact:true}).count(),0);
 viewportResume.resolve();await cell('D200').filter({hasText:'existing distant value'}).waitFor();await page.unroute('**/api/cells/viewport',delayedViewport);
 await grid().press('F2');input=page.getByRole('textbox',{name:'Edit D200',exact:true});assert.equal(await input.inputValue(),'existing distant value');await input.press('Escape');await go('A1');
 await edit('B1','=A1*2');await edit('C1','needle');await edit('D1','Needle extra');
 // Add a second sheet through the UI and seed a matching literal.
 page.once('dialog',d=>d.accept('Search Other'));await page.getByRole('button',{name:'Add worksheet',exact:true}).click();await page.getByRole('grid',{name:'Search Other worksheet'}).waitFor();await edit('A1','needle');
 await page.getByRole('button',{name:'Sheet1',exact:true}).click();await page.getByRole('grid',{name:'Sheet1 worksheet'}).waitFor();
 await page.getByRole('button',{name:'Find (Ctrl+F)',exact:true}).click();let panel=page.getByRole('complementary',{name:'Find in workbook'});const query=()=>panel.getByRole('textbox',{name:'Find text or number'});
 const find=async text=>{await query().fill(text);await panel.getByRole('button',{name:'Find all',exact:true}).click();};
 await find('needle');await panel.getByText('2 matches',{exact:true}).waitFor();await panel.getByLabel('Match entire cell',{exact:true}).check();await panel.getByRole('button',{name:'Find all',exact:true}).click();await panel.getByText('1 matches',{exact:true}).waitFor();
 await panel.getByLabel('Find scope',{exact:true}).selectOption('workbook');await panel.getByRole('button',{name:'Find all',exact:true}).click();await panel.getByText('2 matches',{exact:true}).waitFor();await panel.getByRole('button').filter({hasText:'Search Other!A1'}).click();await page.getByRole('grid',{name:'Search Other worksheet'}).waitFor();await selected('A1');
 await panel.getByLabel('Find look in',{exact:true}).selectOption('formulas');await find('=A1*2');await panel.getByText('1 matches',{exact:true}).waitFor();await panel.getByRole('button').filter({hasText:'Sheet1!B1'}).click();await page.getByRole('grid',{name:'Sheet1 worksheet'}).waitFor();await selected('B1');
 await panel.getByLabel('Find look in',{exact:true}).selectOption('values');await find('28');await panel.getByText('1 matches',{exact:true}).waitFor();
 // Old results must not overwrite a changed query, or a newly opened panel.
 const stalePayload={matches:[],total_matches:999,next_offset:null,scanned_cells:1,truncated:false,warnings:[]};
 for(const close of [false,true]) {
   const started=deferred(),resume=deferred(),done=deferred();
   const stale=async route=>{started.resolve();await resume.promise;await route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(stalePayload)});done.resolve();};await page.route('**/api/cells/find',stale);
   await find('old request');await started.promise;
   if(close){await panel.getByRole('button',{name:'Close find'}).click();await page.getByRole('button',{name:'Find (Ctrl+F)',exact:true}).click();panel=page.getByRole('complementary',{name:'Find in workbook'});}else{await query().fill('new request');}
   const staleResponse=page.waitForResponse(response=>response.url().endsWith('/api/cells/find'));
   resume.resolve();await done.promise;await staleResponse;await page.unroute('**/api/cells/find',stale);
   await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
   assert.equal(await panel.getByText('999 matches',{exact:true}).count(),0);
   // A subsequent real request establishes that the stale response and its queued React work completed.
   await find('needle');await panel.getByText('2 matches',{exact:true}).waitFor();assert.equal(await panel.getByText('999 matches',{exact:true}).count(),0);
 }
 await query().press('Escape');assert.equal(await panel.count(),0);
 await page.screenshot({path:path.join(artifacts,'usability.png'),fullPage:true});assert.deepEqual(errors,[]);
 console.log('PASS keyboard commit/navigation, Escape, failed retry, duplicate delayed commit, Go to, Find values/formulas/exact/workbook, stale query and close/reopen.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
