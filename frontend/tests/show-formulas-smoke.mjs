// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/show-formulas-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const seed=async(row,col,raw_input)=>{const r=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:'Sheet1',row,col,raw_input}});assert(r.ok(),await r.text());};
 await seed(0,0,'2');await seed(0,1,'=A1*2');await seed(0,2,'=1/0');await seed(0,3,'=SEQUENCE(2,1)');await seed(0,4,'literal text');await seed(199,0,'=2+3');await seed(0,25,'=4+5');
 await page.goto(base);const grid=()=>page.getByRole('grid'),cell=name=>page.getByRole('gridcell',{name,exact:true}),toggle=page.getByLabel('Show formulas',{exact:true}),address=page.getByRole('textbox',{name:'Go to cell',exact:true}),formula=page.getByRole('textbox',{name:'Formula input',exact:true});
 const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const go=async name=>{await address.fill(name);await address.press('Enter');await cell(name).and(page.locator('[aria-selected="true"]')).waitFor();await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);};
 const text=async(name,expected)=>{await cell(name).filter({hasText:expected}).waitFor();assert.equal(await cell(name).innerText(),expected);};
 await grid().waitFor();await text('B1','4');const initialWidth=(await cell('B1').boundingBox()).width;
 // Establish the existing formula-aware clipboard representation before changing presentation.
 const copyReady=page.waitForResponse(r=>r.url().endsWith('/api/ranges/capture'));await go('B1');await(await copyReady).finished();
 const copySelection=async()=>{await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));return grid().evaluate(el=>{const clipboardData=new DataTransfer();el.dispatchEvent(new ClipboardEvent('copy',{bubbles:true,clipboardData}));return clipboardData.getData('text/plain');});};
 const baselineCopy=await copySelection();assert.equal(baselineCopy,'=A1*2');
 await page.getByRole('tab',{name:'View',exact:true}).click();await toggle.click();await idle();assert.equal(await toggle.isChecked(),true);await text('B1','=A1*2');await text('C1','=1/0');await text('D1','=SEQUENCE(2,1)');await text('D2','2');await text('E1','literal text');assert.equal((await cell('B1').boundingBox()).width,initialWidth);assert.match(await cell('B1').getAttribute('title'),/Evaluated value: 4/);assert.match(await cell('C1').getAttribute('title'),/Evaluated value: #DIV\/0!/);
 const summary=page.getByRole('region',{name:'Selection summary'});await summary.getByLabel('Selection sum',{exact:true}).filter({hasText:'4'}).waitFor();assert.equal(await summary.getByLabel('Selection sum',{exact:true}).innerText(),'4');
 // Presentation must not change the existing formula-aware clipboard representation.
 assert.equal(await copySelection(),baselineCopy);
 await page.getByRole('tab',{name:'Home',exact:true}).click();await page.getByRole('button',{name:'Find (Ctrl+F)',exact:true}).click();const find=page.getByRole('complementary',{name:'Find in workbook'});await find.getByLabel('Find text or number').fill('4');await find.getByLabel('Match entire cell',{exact:true}).check();await find.getByRole('button',{name:'Find all',exact:true}).click();await find.getByRole('button').filter({hasText:'Sheet1!B1'}).waitFor();await find.getByLabel('Find look in').selectOption('formulas');await find.getByLabel('Find text or number').fill('=A1*2');await find.getByRole('button',{name:'Find all',exact:true}).click();await find.getByRole('button').filter({hasText:'Sheet1!B1'}).waitFor();await find.getByRole('button',{name:'Close find'}).click();
 await page.getByRole('tab',{name:'View',exact:true}).click();
 // Neither a grid draft nor a formula-bar draft may be committed by the label.
 // Force only bypasses Playwright’s enabled wait: intentionally attempt a physical
 // click on the disabled control’s label without enabling it or dispatching change.
 let cellWrites=0;const countWrite=r=>{if(r.url().endsWith('/api/cells/set'))cellWrites++;};page.on('request',countWrite);
 await cell('B1').dblclick();const editor=page.getByRole('textbox',{name:'Edit B1',exact:true});await editor.fill('=A1*3');assert.equal(await toggle.isDisabled(),true);await page.getByText('Show formulas',{exact:true}).click({force:true});assert.equal(cellWrites,0);assert.equal(await editor.inputValue(),'=A1*3');assert.equal(await toggle.isChecked(),true);await editor.press('Escape');
 await formula.fill('=A1*4');assert.equal(await toggle.isDisabled(),true);await page.getByText('Show formulas',{exact:true}).click({force:true});assert.equal(cellWrites,0);assert.equal(await formula.inputValue(),'=A1*4');await formula.press('Escape');page.off('request',countWrite);
 await page.getByLabel('Freeze top row',{exact:true}).click();await idle();await page.getByLabel('Freeze first column',{exact:true}).click();await idle();await go('Z200');await text('A200','=2+3');await text('Z1','=4+5');
 page.once('dialog',d=>d.accept('Other view'));await page.getByRole('button',{name:'Add worksheet',exact:true}).click();await page.getByRole('grid',{name:'Other view worksheet'}).waitFor();await idle();assert.equal(await toggle.isChecked(),false);await page.getByRole('button',{name:'Sheet1',exact:true}).click();await idle();assert.equal(await toggle.isChecked(),true);
 const reject=route=>route.fulfill({status:409,body:'Synthetic formula view failure'});await page.route('**/api/sheets/show-formulas',reject);await toggle.click();await page.getByText(/Error: Synthetic formula view failure/).waitFor();await idle();assert.equal(await toggle.isChecked(),true);await page.unroute('**/api/sheets/show-formulas',reject);
 const saved=path.join(artifacts,'formula-view.gridloom');page.once('dialog',d=>d.accept(saved));await page.getByRole('tab',{name:'File',exact:true}).click();await page.getByRole('button',{name:'Save',exact:true}).click();await page.getByText(`Saved workbook to ${saved}`,{exact:true}).waitFor();await page.getByRole('tab',{name:'View',exact:true}).click();await toggle.click();await idle();await text('B1','4');
 await page.getByRole('tab',{name:'File',exact:true}).click();await page.getByRole('button',{name:'Open',exact:true}).click();const replacement=page.getByRole('dialog',{name:'Replace current workbook?',exact:true});page.once('dialog',d=>d.accept(saved));await replacement.getByRole('button',{name:'Discard and continue',exact:true}).click();await page.getByText(`Opened workbook from ${saved}`,{exact:true}).waitFor();await idle();await page.getByRole('tab',{name:'View',exact:true}).click();assert.equal(await toggle.isChecked(),true);await text('B1','=A1*2');await page.screenshot({path:path.join(artifacts,'formula-view.png'),fullPage:true});
 await toggle.click();await idle();await text('B1','4');await text('C1','#DIV/0!');assert.deepEqual(errors,[]);console.log('PASS formula presentation for mixed/error/spill/frozen cells, unchanged widths/summary/copy/Find, draft guards, isolated sheet settings, failure and save/reopen.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
