// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/rounding-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const max='79228162514264337593543950335',tiny='0.0000000000000000000000000001',minDigits='-9223372036854775808',maxDigits='9223372036854775807';
 const seed=async(row,raw_input)=>{const response=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:'Sheet1',row,col:0,raw_input}});assert(response.ok(),await response.text());};await seed(0,max);await seed(1,tiny);await seed(2,'-'+tiny);await seed(3,'123.456');await seed(4,'2.5');
 await page.goto(base);const grid=()=>page.getByRole('grid'),cell=name=>page.getByRole('gridcell',{name,exact:true}),address=page.getByRole('textbox',{name:'Go to cell',exact:true});const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const go=async name=>{await address.fill(name);await address.press('Enter');await cell(name).and(page.locator('[aria-selected="true"]')).waitFor();await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);};
 const edit=async(name,value)=>{await go(name);await grid().focus();await grid().press('F2');const input=page.getByRole('textbox',{name:`Edit ${name}`,exact:true});await input.fill(value);await input.press('Enter');await idle();};
 const viewport=async()=>{const response=await page.request.post(`${base}/api/cells/viewport`,{data:{sheet_name:'Sheet1',start_row:0,start_col:0,end_row:15,end_col:7,overscan:0}});assert(response.ok(),await response.text());return(await response.json()).cells;};
 const expect=async(name,value)=>{await go(name);await cell(name).filter({hasText:value}).waitFor();assert.equal(await cell(name).innerText(),value);const raw=(await viewport())[name].raw_value;if(value.startsWith('#'))assert.equal(raw.type,'Error');else{assert.equal(raw.type,'Decimal');assert.equal(raw.value,value,'Exact decimal strings must never round-trip through JavaScript Number');}};
 const cases=[
  ['C1','=ROUND(A4,19)','123.456'],
  ['C2',`=ROUNDUP(A4,${maxDigits})`,'123.456'],
  ['C3',`=ROUNDDOWN(A4,${minDigits})`,'0'],
  ['C4',`=TRUNC(A4,${minDigits})`,'0'],
  ['C5',`=ROUNDUP(A4,${minDigits})`,'#NUM!'],
  ['C6','=ROUND(A1,1)',max],
  ['C7','=ROUNDUP(A1,-1)','#NUM!'],
  ['C8','=ROUNDDOWN(A1,-1)','79228162514264337593543950330'],
  ['C9','=ROUND(A1,-29)','#NUM!'],
  ['C10','=ROUND(A2,-1)','0'],
  ['C11','=ROUNDUP(A2,-1)','10'],
  ['C12','=ROUNDUP(A3,-1)','-10'],
  ['C13','=TRUNC(A3,-1)','0'],
  ['E1','=ROUND(A5,0)','2'],
  ['E2','=ROUND(3.5,0)','4'],
  ['E3','=ROUND(-2.5,0)','-2'],
  ['E4','=ROUND(-3.5,0)','-4'],
  ['E5','=ROUND(1.25,1)','1.2'],
  ['E6','=ROUND(-1.25,1)','-1.2'],
  ['E7','=ROUNDUP(A4,2)','123.46'],
  ['E8','=ROUNDDOWN(A4,2)','123.45'],
  ['E9','=TRUNC(-A4,2)','-123.45'],
  ['G1','=C1+E1','125.456'],
  ['H1','=ROUND(A4,4294967296)','123.456'],
  ['H2','=ROUNDUP(A4,19)','123.456'],
  ['H3','=ROUNDDOWN(A4,19)','123.456'],
  ['H4','=TRUNC(A4,19)','123.456'],
  ['H5','=ROUNDUP(A1,1)',max],
  ['H6','=ROUNDDOWN(A1,1)',max],
  ['H7','=TRUNC(A1,1)',max],
 ];
 await grid().waitFor();for(const[name,formula,value]of cases){await edit(name,formula);await expect(name,value);}
 // Extreme formula errors are ordinary cell errors, not request/server failures.
 assert((await page.request.get(`${base}/api/workbook`)).ok());await edit('A4','987.654');await expect('C1','987.654');await expect('G1','989.654');await edit('A5','3.5');await expect('E1','4');await expect('G1','991.654');await expect('C5','#NUM!');
 const saved=path.join(artifacts,'exact-rounding.gridloom');page.once('dialog',d=>d.accept(saved));await page.getByRole('button',{name:'Save',exact:true}).click();await page.getByText(`Saved workbook to ${saved}`,{exact:true}).waitFor();
 await edit('A4','1');await expect('G1','5');await page.getByRole('button',{name:'Open',exact:true}).click();const replacement=page.getByRole('dialog',{name:'Replace current workbook?',exact:true});page.once('dialog',d=>d.accept(saved));await replacement.getByRole('button',{name:'Discard and continue',exact:true}).click();await page.getByText(`Opened workbook from ${saved}`,{exact:true}).waitFor();await idle();
 await expect('G1','991.654');await expect('C6',max);await expect('C7','#NUM!');await expect('C11','10');await expect('C12','-10');assert((await page.request.get(`${base}/api/workbook`)).ok());
 await page.screenshot({path:path.join(artifacts,'rounding.png'),fullPage:true});assert.deepEqual(errors,[]);console.log('PASS exact decimal rounding with extreme digit arguments, maximum/tiny coefficients, signed half-even ties, explicit overflow errors, dependent updates and save/reopen.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
