// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/product-mod-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const max='79228162514264337593543950335',tiny='0.0000000000000000000000000001';
 const seed=async(row,raw_input)=>{const response=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:'Sheet1',row,col:0,raw_input}});assert(response.ok(),await response.text());};await seed(0,max);await seed(1,tiny);await seed(2,'1.25');await seed(3,'0.2');
 await page.goto(base);const grid=()=>page.getByRole('grid'),cell=name=>page.getByRole('gridcell',{name,exact:true}),address=page.getByRole('textbox',{name:'Go to cell',exact:true});const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const go=async name=>{await address.fill(name);await address.press('Enter');await cell(name).and(page.locator('[aria-selected="true"]')).waitFor();await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);};
 const edit=async(name,value)=>{await go(name);await grid().focus();await grid().press('F2');const input=page.getByRole('textbox',{name:`Edit ${name}`,exact:true});await input.fill(value);await input.press('Enter');await idle();};
 const viewport=async()=>{const response=await page.request.post(`${base}/api/cells/viewport`,{data:{sheet_name:'Sheet1',start_row:0,start_col:0,end_row:15,end_col:7,overscan:0}});assert(response.ok(),await response.text());return(await response.json()).cells;};
 // Decimal scale may retain harmless trailing zeros; compare decimal characters, never JS numbers.
 const canonical=value=>value.includes('.')?value.replace(/0+$/,'').replace(/\.$/,''):value;
 const expect=async(name,value)=>{await go(name);await cell(name).filter({hasText:value}).waitFor();assert.equal(canonical(await cell(name).innerText()),value);const raw=(await viewport())[name].raw_value;if(value.startsWith('#'))assert.equal(raw.type,'Error');else{assert.equal(raw.type,'Decimal');assert.equal(canonical(raw.value),value,'Exact decimal strings must never round-trip through JavaScript Number');}};
 const cases=[
  ['C1','=PRODUCT(A1,2)','#NUM!'],
  ['C2','=PRODUCT(-A1,2)','#NUM!'],
  ['C3','=PRODUCT(A3:A4)','0.25'],
  ['C4','=PRODUCT(A1,1)',max],
  ['C5','=PRODUCT(1/0,2)','#DIV/0!'],
  ['C6','=PRODUCT(A2,2)','0.0000000000000000000000000002'],
  ['E1','=MOD(A1,10)','5'],
  ['E2','=MOD(-A1,10)','5'],
  ['E3','=MOD(A1,-10)','-5'],
  ['E4','=MOD(-A1,-10)','-5'],
  ['E5','=MOD(1,A2)','0'],
  ['E6','=MOD(A2,1)',tiny],
  ['E7','=MOD(0.3,0.1)','0'],
  ['E8','=MOD(3,0)','#DIV/0!'],
  ['E9','=MOD(1/0,2)','#DIV/0!'],
  ['E10','=MOD(A1,0.1)','0'],
  ['E11','=MOD(A1,A2)','0'],
  ['E12','=MOD(-A1,A2)','0'],
  ['E13','=MOD(-A2,A1)','#NUM!'],
  ['E14','=MOD(-3,2)','1'],
  ['E15','=MOD(3,-2)','-1'],
  ['F1','=MOD(-A2,1)','0.9999999999999999999999999999'],
  ['G1','=PRODUCT(C3,2)','0.5'],
 ];
 await grid().waitFor();for(const[name,formula,value]of cases){await edit(name,formula);await expect(name,value);}
 // Errors remain ordinary cell results, and healthy calculations still work after overflow.
 assert((await page.request.get(`${base}/api/workbook`)).ok());await edit('A3','2.5');await expect('C3','0.5');await expect('G1','1');await expect('C1','#NUM!');
 const saved=path.join(artifacts,'product-mod.tay');page.once('dialog',d=>d.accept(saved));await page.getByRole('tab',{name:'File',exact:true}).click();await page.getByRole('button',{name:'Save',exact:true}).click();await page.getByText(`Saved workbook to ${saved}`,{exact:true}).waitFor();
 await edit('A3','5');await expect('G1','2');await page.getByRole('tab',{name:'File',exact:true}).click();await page.getByRole('button',{name:'Open',exact:true}).click();const replacement=page.getByRole('dialog',{name:'Replace current workbook?',exact:true});page.once('dialog',d=>d.accept(saved));await replacement.getByRole('button',{name:'Discard and continue',exact:true}).click();await page.getByText(`Opened workbook from ${saved}`,{exact:true}).waitFor();await idle();
 await expect('G1','1');await expect('C1','#NUM!');await expect('C4',max);await expect('E1','5');await expect('E5','0');assert((await page.request.get(`${base}/api/workbook`)).ok());
 await page.screenshot({path:path.join(artifacts,'product-mod.png'),fullPage:true});assert.deepEqual(errors,[]);console.log('PASS checked PRODUCT overflow, exact MOD without quotient overflow, signed remainders, propagated errors, healthy dependent recalculation and save/reopen.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
