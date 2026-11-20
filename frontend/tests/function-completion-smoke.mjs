// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/function-completion-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const seed=async(row,raw_input)=>{const response=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:'Sheet1',row,col:0,raw_input}});assert(response.ok(),await response.text());};await seed(0,'1');await seed(1,'2');await seed(2,'3');
 const catalogueReady=page.waitForResponse(response=>response.url().endsWith('/api/functions'));await page.goto(base);const catalogue=await(await catalogueReady).json();assert(catalogue.functions.includes('STDEV.S'));assert(catalogue.functions.includes('ROUND'));assert.deepEqual(catalogue.functions,[...new Set(catalogue.functions)].sort());
 const grid=()=>page.getByRole('grid'),cell=name=>page.getByRole('gridcell',{name,exact:true}),formula=page.getByRole('textbox',{name:'Formula input',exact:true}),list=page.getByRole('listbox',{name:'Function suggestions',exact:true});
 const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const value=async text=>{await page.waitForFunction(({el,text})=>el.value===text,{el:await formula.elementHandle(),text});assert.equal(await formula.inputValue(),text);};
 const caret=async index=>{await formula.focus();await formula.press('Home');for(let i=0;i<index;i++)await formula.press('ArrowRight');};
 await grid().waitFor();await cell('B1').click();let writes=0;page.on('request',r=>{if(r.url().endsWith('/api/cells/set'))writes++;});
 await formula.fill('=S');await list.waitFor();const sOptions=list.getByRole('option');const optionCount=await sOptions.count();for(let i=1;i<optionCount;i++){await formula.press('ArrowDown');await page.waitForFunction(({option})=>option.getAttribute('aria-selected')==='true',{option:await sOptions.nth(i).elementHandle()});}
 const selectedOption=sOptions.nth(optionCount-1);await page.waitForFunction(({menu,option})=>{const m=menu.getBoundingClientRect(),o=option.getBoundingClientRect();return option.getAttribute('aria-selected')==='true'&&o.y>=m.y&&o.bottom<=m.bottom+1;},{menu:await list.elementHandle(),option:await selectedOption.elementHandle()});assert.equal(await selectedOption.getAttribute('aria-selected'),'true');const menuBox=await list.boundingBox(),optionBox=await selectedOption.boundingBox();assert(menuBox&&optionBox);assert(optionBox.y>=menuBox.y&&optionBox.y+optionBox.height<=menuBox.y+menuBox.height+1,'Keyboard choice must scroll into view');
 await page.screenshot({path:path.join(artifacts,'completion-menu.png'),fullPage:true});
 await formula.press('Shift+Tab');await value('=S');assert.equal(writes,0);assert.equal(await formula.evaluate(el=>document.activeElement===el),false,'Shift+Tab should leave the input without accepting');
 await formula.fill('=RO');await list.getByRole('option',{name:'ROUND',exact:true}).waitFor();await formula.press('ArrowDown');await formula.press('Tab');await value('=ROUNDDOWN(');assert.equal(writes,0);
 await formula.fill('=ROUND(1.26,1)');await formula.press('Enter');await idle();assert.equal(await cell('B1').innerText(),'1.3');
 await formula.fill('=STDEV.S');await list.getByRole('option',{name:'STDEV.S',exact:true}).waitFor();const beforeDotted=writes;await formula.press('Enter');await value('=STDEV.S(');assert.equal(writes,beforeDotted,'Accepting a suggestion must not submit a cell');await formula.fill('=STDEV.S(A1:A3)');await formula.press('Enter');await idle();assert.equal(await cell('B1').innerText(),'1');
 await formula.fill('=IF(A1>0,SUX(A1:A3),0)');await caret(11);await list.getByRole('option',{name:'SUM',exact:true}).click();await value('=IF(A1>0,SUM(A1:A3),0)');assert.equal(await formula.evaluate(el=>el.selectionStart),13);await formula.press('Enter');await idle();assert.equal(await cell('B1').innerText(),'6');
 await formula.fill('=RO');await list.waitFor();await formula.press('Escape');assert.equal(await list.count(),0);await value('=RO');await formula.press('Escape');await value('=IF(A1>0,SUM(A1:A3),0)');await formula.fill('=RO');await list.waitFor();await formula.press('Escape');await formula.press('Escape');await value('=IF(A1>0,SUM(A1:A3),0)');
 for(const text of ['="SU',"='SU",'=Table[SU','=Sheet!SU']){await formula.fill(text);assert.equal(await list.count(),0,text);}
 for(const [text,index] of [['=SUM1',4],['=SU!A1',3],['=SUM[Amt]',4],['=SUM$1',4],['=A$1',2]]){await formula.fill(text);await caret(index);assert.equal(await list.count(),0,text);}
 await formula.press('Escape');
 await formula.fill('=SUM(A1)');await caret(3);await list.getByRole('option',{name:'SUM',exact:true}).waitFor();await caret(7);await formula.press('F4');await value('=SUM($A$1)');await formula.press('Escape');
 const reject=route=>route.fulfill({status:409,contentType:'text/plain',body:'Synthetic completion failure'});await page.route('**/api/cells/set',reject);await formula.fill('=RO');await list.getByRole('option',{name:'ROUND',exact:true}).click();await value('=ROUND(');await formula.fill('=ROUND(A1,0)');await formula.press('Enter');await idle();await page.getByText(/Error: Synthetic completion failure/).waitFor();await value('=ROUND(A1,0)');await formula.press('Escape');await page.unroute('**/api/cells/set',reject);
 // The same catalog is available through fx and direct cell typing.
 await cell('B2').click();
 await page.getByRole('button',{name:'Insert function',exact:true}).click();
 const picker=page.getByRole('dialog',{name:'Insert function'});
 await picker.getByRole('textbox',{name:'Search functions'}).fill('STDEV.S');
 await picker.getByRole('button',{name:'STDEV.S',exact:true}).click();
 await value('=STDEV.S(');
 await formula.fill('=SUM(A1:A3,B1)');
 assert.equal(await cell('A2').getAttribute('data-formula-reference'),'1');
 assert.equal(await cell('B1').getAttribute('data-formula-reference'),'2');
 await formula.press('Escape');
 assert.equal(await cell('A2').getAttribute('data-formula-reference'),null);
 await cell('B1').click();
 await grid().press('F2');
 const inline=page.getByRole('textbox',{name:'Edit B1',exact:true});
 await inline.fill('=SU');const sumOption=list.getByRole('option',{name:'SUM',exact:true});await sumOption.waitFor();
 for(let i=0,n=await list.getByRole('option').count();i<n&&await sumOption.getAttribute('aria-selected')!=='true';i++)await inline.press('ArrowDown');
 assert.equal(await sumOption.getAttribute('aria-selected'),'true');
 await inline.press('Tab');assert.equal(await inline.inputValue(),'=SUM(');
 const beforeInline=writes;
 await inline.press('ArrowLeft');assert.equal(await inline.inputValue(),'=SUM(A1');
 await inline.press('Shift+ArrowDown');await inline.press('Shift+ArrowDown');
 assert.equal(await inline.inputValue(),'=SUM(A1:A3');
 assert.equal(await cell('A2').getAttribute('data-formula-reference'),'1');
 assert.equal(await cell('B1').getAttribute('aria-selected'),'true');
 assert.equal(writes,beforeInline,'Reference picking must never write or change the edit target');
 await inline.press('End');await inline.press(')');
 await inline.press('Enter');await idle();assert.equal(await cell('B1').innerText(),'6');
 assert.equal(await cell('A2').getAttribute('data-formula-reference'),null);
 // Catalogue availability is optional: ordinary manual formula entry still works.
 await page.route('**/api/functions',route=>route.fulfill({status:503,body:'Unavailable'}));await page.reload();await grid().waitFor();await cell('B1').click();await formula.fill('=SUM(A1:A3)');await formula.press('Enter');await idle();assert.equal(await cell('B1').innerText(),'6');assert.equal(await list.count(),0);
 await page.screenshot({path:path.join(artifacts,'completion.png'),fullPage:true});assert.deepEqual(errors,[]);console.log('PASS authoritative/dotted/nested autocomplete, keyboard acceptance without submit, suffix/caret preservation, lexical exclusions, Escape/failed draft retention, unavailable-catalogue fallback.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)writeFileSync(path.join(artifacts,'completion-state.json'),JSON.stringify(await page.evaluate(()=>({active:document.activeElement?.outerHTML,input:(()=>{const el=document.querySelector('input[aria-label="Formula input"]');return el?{value:el.value,start:el.selectionStart,end:el.selectionEnd}:null;})(),menu:(()=>{const el=document.querySelector('[role="listbox"]');return el?{box:el.getBoundingClientRect().toJSON(),scrollTop:el.scrollTop,clientHeight:el.clientHeight,scrollHeight:el.scrollHeight}:null;})(),options:[...document.querySelectorAll('[role="option"]')].map(el=>({name:el.getAttribute('aria-label'),selected:el.getAttribute('aria-selected'),box:el.getBoundingClientRect().toJSON()}))})).catch(e=>({diagnosticError:String(e)})),null,2));if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
