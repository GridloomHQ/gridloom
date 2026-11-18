// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/formula-reference-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const seed=async(row,col,raw_input)=>{const response=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:'Sheet1',row,col,raw_input}});assert(response.ok(),await response.text());};
 await seed(0,0,'2');await seed(1,0,'3');await seed(2,0,'4');await seed(0,3,'10');
 await page.goto(base);const grid=()=>page.getByRole('grid');const cell=name=>page.getByRole('gridcell',{name,exact:true});const formula=page.getByRole('textbox',{name:'Formula input',exact:true});const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const setCaret=async(input,start,end=start)=>input.evaluate((el,{start,end})=>el.setSelectionRange(start,end),{start,end});
 const exactValue=async(input,value)=>{await page.waitForFunction(({el,value})=>el.value===value,{el:await input.elementHandle(),value});assert.equal(await input.inputValue(),value);};
 const raw=async(name)=>{const response=await page.request.post(`${base}/api/cells/viewport`,{data:{sheet_name:'Sheet1',start_row:0,start_col:0,end_row:3,end_col:4,overscan:0}});return(await response.json()).cells[name];};
 await grid().waitFor();let writes=0;page.on('request',r=>{if(r.url().endsWith('/api/cells/set'))writes++;});
 await cell('B1').dblclick();let editor=page.getByRole('textbox',{name:'Edit B1',exact:true});await editor.fill('=A1*D1');await setCaret(editor,6);
 for(const expected of ['=A1*$D$1','=A1*D$1','=A1*$D1','=A1*D1']){await editor.press('F4');await exactValue(editor,expected);assert.equal(await editor.evaluate(el=>el.value.slice(el.selectionStart,el.selectionEnd)),expected.slice(4));}
 assert.equal(writes,0,'Reference cycling must only change the draft');await editor.press('F4');await editor.press('Enter');await idle();assert.equal((await raw('B1')).raw_input,'=A1*$D$1');assert.equal(await cell('B1').innerText(),'20');
 // Lock an assumption once, then fill down and right with the ordinary controls.
 await cell('B1').click();await cell('B3').click({modifiers:['Shift']});await page.getByRole('button',{name:'Fill down',exact:true}).click();await idle();assert.equal((await raw('B2')).raw_input,'=A2*$D$1');assert.equal((await raw('B3')).raw_input,'=A3*$D$1');assert.equal(await cell('B3').innerText(),'40');
 await cell('B1').click();await cell('C1').click({modifiers:['Shift']});await page.getByRole('button',{name:'Fill right',exact:true}).click();await idle();assert.equal((await raw('C1')).raw_input,'=B1*$D$1');assert.equal(await cell('C1').innerText(),'200');await page.getByRole('button',{name:'Undo',exact:true}).click();await idle();assert.equal((await raw('C1'))?.raw_input??null,null);assert.equal(await cell('B3').innerText(),'40');
 // Formula bar uses the same selected-token cycle; Escape restores stored input.
 await cell('B1').click();await formula.fill('=A1*D1');await setCaret(formula,4,6);const beforeCycle=writes;
 for(const expected of ['=A1*$D$1','=A1*D$1','=A1*$D1','=A1*D1']){await formula.press('F4');await exactValue(formula,expected);}
 assert.equal(writes,beforeCycle);await formula.press('Escape');await exactValue(formula,'=A1*$D$1');
 await formula.fill('="D1"');await setCaret(formula,3);await formula.press('F4');await exactValue(formula,'="D1"');await formula.press('Escape');
 // Failed commits keep the transformed draft available, and F4 remains usable.
 const reject=route=>route.fulfill({status:409,contentType:'text/plain',body:'Synthetic reference write failure'});await page.route('**/api/cells/set',reject);
 await formula.fill('=A1*D1');await setCaret(formula,6);await formula.press('F4');await formula.press('Enter');await idle();await page.getByText(/Error: Synthetic reference write failure/).waitFor();await exactValue(formula,'=A1*$D$1');await setCaret(formula,4,8);await formula.press('F4');await exactValue(formula,'=A1*D$1');await formula.press('Escape');
 await cell('B1').dblclick();editor=page.getByRole('textbox',{name:'Edit B1',exact:true});await editor.fill('=A1*D1');await setCaret(editor,6);await editor.press('F4');await editor.press('Enter');await idle();await exactValue(editor,'=A1*$D$1');await setCaret(editor,4,8);await editor.press('F4');await exactValue(editor,'=A1*D$1');await editor.press('Escape');await page.unroute('**/api/cells/set',reject);assert.equal((await raw('B1')).raw_input,'=A1*$D$1');
 // Huge selections must fail before matrix allocation or any bulk-write request.
 const address=page.getByRole('textbox',{name:'Go to cell',exact:true});await address.fill('K1000');await address.press('Enter');await cell('K1000').and(page.locator('[aria-selected="true"]')).waitFor();await grid().focus();await grid().press('Control+Shift+Home');
 const bulk=[];const watchBulk=request=>{if(/\/api\/ranges\/(edit|fill|format|format-attribute)$/.test(request.url()))bulk.push(request.url());};page.on('request',watchBulk);
 await grid().press('Delete');await page.getByRole('status').filter({hasText:'Select at most 10,000 cells for this operation'}).waitFor();
 await page.getByRole('button',{name:'B',exact:true}).click();await page.getByRole('button',{name:'Fill down',exact:true}).click();await page.getByRole('button',{name:'Fill right',exact:true}).click();assert.deepEqual(bulk,[]);page.off('request',watchBulk);
 await address.fill('A4');await address.press('Enter');await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);await cell('A4').dblclick();editor=page.getByRole('textbox',{name:'Edit A4',exact:true});await editor.fill('7');await editor.press('Enter');await idle();assert.equal((await raw('A4')).raw_input,'7');
 await page.screenshot({path:path.join(artifacts,'formula-reference.png'),fullPage:true});assert.deepEqual(errors,[]);console.log('PASS draft-only F4 in both editors, repeated selected-token cycling, Escape/failed drafts, locked-assumption fill down/right and undo.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
