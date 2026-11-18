// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/save-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(base);const grid=()=>page.getByRole('grid');const cell=name=>page.getByRole('gridcell',{name,exact:true,includeHidden:true});const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const save=page.getByRole('button',{name:'Save',exact:true}),saveAs=page.getByRole('button',{name:'Save As',exact:true});const replacement=page.getByRole('dialog',{name:'Replace current workbook?',exact:true});
 const edit=async(name,value)=>{await cell(name).dblclick();const input=page.getByRole('textbox',{name:`Edit ${name}`,exact:true});await input.fill(value);await input.press('Enter');await idle();};
 await grid().waitFor();await edit('A1','11');
 const first=path.join(artifacts,'first.gridloom'),second=path.join(artifacts,'second.gridloom');
 const writes=[];page.on('request',request=>{if(request.url().endsWith('/api/workbook/save'))writes.push(request.postDataJSON().path);});
 page.once('dialog',d=>d.accept(first));await save.click();await page.getByText(`Saved workbook to ${first}`,{exact:true}).waitFor();assert.deepEqual(writes,[first]);
 await edit('A1','12');await save.click();await idle();assert.deepEqual(writes,[first,first]);
 page.once('dialog',d=>d.accept(second));await saveAs.click();await page.getByText(`Saved workbook to ${second}`,{exact:true}).waitFor();await idle();
 await grid().focus();await grid().press('Control+s');await idle();assert.deepEqual(writes,[first,first,second,second]);
 // Unsubmitted FormulaBar content is not silently saved or cleared on failure.
 const formula=page.getByRole('textbox',{name:'Formula input',exact:true});await formula.fill('unsaved formula draft');const beforeDraft=writes.length;await save.click();await page.getByText(/Finish or cancel the cell edit/).waitFor();assert.equal(writes.length,beforeDraft);assert.equal(await formula.inputValue(),'unsaved formula draft');
 const rejectCell=route=>route.fulfill({status:409,contentType:'text/plain',body:'Synthetic formula failure'});await page.route('**/api/cells/set',rejectCell);await formula.press('Enter');await idle();assert.equal(await formula.inputValue(),'unsaved formula draft');await save.click();assert.equal(writes.length,beforeDraft);await page.unroute('**/api/cells/set',rejectCell);await formula.press('Escape');
 await cell('A1').dblclick();let editor=page.getByRole('textbox',{name:'Edit A1',exact:true});await editor.fill('unsaved grid draft');await save.click();assert.equal(writes.length,beforeDraft);assert.equal(await editor.inputValue(),'unsaved grid draft');await editor.press('Escape');
 // Cancel replacement cannot mutate. Background is inert, including keyboard tabs.
 await page.getByRole('button',{name:'New',exact:true}).click();await replacement.waitFor();await replacement.getByRole('button',{name:'Cancel',exact:true}).focus();await page.keyboard.press('Tab');assert.equal(await replacement.getByRole('button',{name:'Save and continue',exact:true}).evaluate(el=>el===document.activeElement),true);await replacement.getByRole('button',{name:'Cancel',exact:true}).click();assert.equal(await cell('A1').innerText(),'12');
 // A failed Save As keeps the previous path; failed Save-and-continue keeps workbook.
 const rejectSave=route=>route.fulfill({status:500,contentType:'text/plain',body:'Synthetic disk failure'});await page.route('**/api/workbook/save',rejectSave);
 page.once('dialog',d=>d.accept(path.join(artifacts,'failed.gridloom')));await saveAs.click();await page.getByText(/Failed to save: Synthetic disk failure/).waitFor();
 await page.getByRole('button',{name:'New',exact:true}).click();await replacement.getByRole('button',{name:'Save and continue',exact:true}).click();await page.getByText(/Failed to save: Synthetic disk failure/).waitFor();await idle();assert.equal(await replacement.count(),1);assert.equal(await cell('A1').innerText(),'12');await replacement.getByRole('button',{name:'Cancel',exact:true}).click();await page.unroute('**/api/workbook/save',rejectSave);
 await save.click();await idle();assert.equal(writes.at(-1),second);
 // While save is pending, shortcuts and repeated clicks cannot queue a second save.
 const started=deferred(),resume=deferred();let delayed=0;const delaySave=async route=>{delayed++;started.resolve();await resume.promise;await route.continue();};await page.route('**/api/workbook/save',delaySave);await grid().focus();await grid().press('Control+s');await started.promise;await grid().press('Control+s');assert.equal(delayed,1);resume.resolve();await idle();await page.unroute('**/api/workbook/save',delaySave);
 await page.getByRole('button',{name:'New',exact:true}).click();await replacement.getByRole('button',{name:'Save and continue',exact:true}).click();await page.getByText('Created new workbook',{exact:true}).waitFor();await idle();assert.equal(await cell('A1').innerText(),'');
 // No known path in new workbook: canceling Save aborts replacement.
 await page.getByRole('button',{name:'Open',exact:true}).click();page.once('dialog',d=>d.dismiss());await replacement.getByRole('button',{name:'Save and continue',exact:true}).click();assert.equal(await replacement.count(),1);
 page.once('dialog',d=>d.accept(second));await replacement.getByRole('button',{name:'Discard and continue',exact:true}).click();await page.getByText(`Opened workbook from ${second}`,{exact:true}).waitFor();await idle();assert.equal(await cell('A1').innerText(),'12');
 await page.screenshot({path:path.join(artifacts,'save.png'),fullPage:true});assert.deepEqual(errors,[]);console.log('PASS known-path Save, Save As, shortcut, failed path retention, draft guards, replacement cancel/save/discard, serialized save.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
