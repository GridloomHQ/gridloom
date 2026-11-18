// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/sumproduct-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const max='79228162514264337593543950335';
 const seed=async(row,col,raw_input)=>{const response=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:'Sheet1',row,col,raw_input}});assert(response.ok(),await response.text());};
 for(const [row,col,value] of [[0,0,max],[1,0,max],[0,1,'2'],[1,1,'1'],[3,0,'1.25'],[4,0,'0.2'],[3,1,'0.2'],[4,1,'1.25'],[0,3,'1'],[0,4,'2'],[1,3,'3'],[3,3,'1'],[3,4,'2'],[4,3,'3'],[4,4,'4'],[6,0,'=1/0'],[6,1,'0'],[7,0,'ordinary text'],[7,1,'=1/0']])await seed(row,col,value);
 await page.goto(base);const grid=()=>page.getByRole('grid'),cell=name=>page.getByRole('gridcell',{name,exact:true}),address=page.getByRole('textbox',{name:'Go to cell',exact:true});const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const go=async name=>{await address.fill(name);await address.press('Enter');await cell(name).and(page.locator('[aria-selected="true"]')).waitFor();await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);};
 const edit=async(name,value)=>{await go(name);await grid().focus();await grid().press('F2');const input=page.getByRole('textbox',{name:`Edit ${name}`,exact:true});await input.fill(value);await input.press('Enter');await idle();};
 const viewport=async()=>{const response=await page.request.post(`${base}/api/cells/viewport`,{data:{sheet_name:'Sheet1',start_row:0,start_col:0,end_row:15,end_col:7,overscan:0}});assert(response.ok(),await response.text());return(await response.json()).cells;};
 // Decimal scale may retain harmless trailing zeros; compare decimal characters, never JS numbers.
 const canonical=value=>value.includes('.')?value.replace(/0+$/,'').replace(/\.$/,''):value;
 const expect=async(name,value)=>{await go(name);await cell(name).filter({hasText:value}).waitFor();assert.equal(canonical(await cell(name).innerText()),value);const raw=(await viewport())[name].raw_value;if(value.startsWith('#'))assert.equal(raw.type,'Error');else{assert.equal(raw.type,'Decimal');assert.equal(canonical(raw.value),value,'Exact decimal strings must never round-trip through JavaScript Number');}};
 const cases=[
  ['C1','=SUMPRODUCT(A1:A2,B1:B2)','#NUM!'],
  ['C2','=SUMPRODUCT(A1:A2)','#NUM!'],
  ['C3','=SUMPRODUCT(A4:A5,B4:B5)','0.5'],
  ['C4','=SUMPRODUCT(D1:E1,D1:D2)','#VALUE!'],
  ['C5','=SUMPRODUCT(A7,B7)','#DIV/0!'],
  ['C6','=SUMPRODUCT(A8,B8)','#DIV/0!'],
  ['C7','=SUMPRODUCT(2,3)','6'],
  ['C8','=SUMPRODUCT(2,A4:A5)','#VALUE!'],
  ['C9','=SUMPRODUCT(A4,B4)','0.25'],
  ['C10','=SUMPRODUCT(A1,1)',max],
  ['C11','=SUMPRODUCT(A8,B1)','0'],
  ['C12','=SUMPRODUCT(B7,A7)','#DIV/0!'],
  ['C13','=SUMPRODUCT(D4:E5,D4:E5)','30'],
  ['G1','=PRODUCT(C3,2)','1'],
 ];
 await grid().waitFor();for(const[name,formula,value]of cases){await edit(name,formula);await expect(name,value);}
 // Overflow and invalid input remain cell errors, without compromising later calculations.
 assert((await page.request.get(`${base}/api/workbook`)).ok());await edit('A4','2.5');await expect('C3','0.75');await expect('G1','1.5');await expect('C1','#NUM!');
 const saved=path.join(artifacts,'sumproduct.gridloom');page.once('dialog',d=>d.accept(saved));await page.getByRole('button',{name:'Save',exact:true}).click();await page.getByText(`Saved workbook to ${saved}`,{exact:true}).waitFor();
 await edit('A4','5');await expect('G1','2.5');await page.getByRole('button',{name:'Open',exact:true}).click();const replacement=page.getByRole('dialog',{name:'Replace current workbook?',exact:true});page.once('dialog',d=>d.accept(saved));await replacement.getByRole('button',{name:'Discard and continue',exact:true}).click();await page.getByText(`Opened workbook from ${saved}`,{exact:true}).waitFor();await idle();
 await expect('G1','1.5');await expect('C1','#NUM!');await expect('C2','#NUM!');await expect('C4','#VALUE!');await expect('C5','#DIV/0!');await expect('C10',max);assert((await page.request.get(`${base}/api/workbook`)).ok());
 await page.screenshot({path:path.join(artifacts,'sumproduct.png'),fullPage:true});assert.deepEqual(errors,[]);console.log('PASS SUMPRODUCT rectangular dimensions, propagated errors even paired with text/zero, product and sum overflow, exact dependent recalculation and save/reopen.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
