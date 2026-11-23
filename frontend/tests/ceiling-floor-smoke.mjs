// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/ceiling-floor-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const max='79228162514264337593543950335',tiny='0.0000000000000000000000000001';
 const seed=async(row,raw_input)=>{const response=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:'Sheet1',row,col:0,raw_input}});assert(response.ok(),await response.text());};
 await seed(0,max);await seed(1,tiny);await seed(2,'-'+tiny);await seed(3,'4.3');
 await page.goto(base);const grid=()=>page.getByRole('grid'),cell=name=>page.getByRole('gridcell',{name,exact:true}),address=page.getByRole('textbox',{name:'Go to cell',exact:true});const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const go=async name=>{await address.fill(name);await address.press('Enter');await cell(name).and(page.locator('[aria-selected="true"]')).waitFor();await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);};
 const edit=async(name,value)=>{await go(name);await grid().focus();await grid().press('F2');const input=page.getByRole('textbox',{name:`Edit ${name}`,exact:true});await input.fill(value);await input.press('Enter');await idle();};
 const viewport=async()=>{const response=await page.request.post(`${base}/api/cells/viewport`,{data:{sheet_name:'Sheet1',start_row:0,start_col:0,end_row:15,end_col:7,overscan:0}});assert(response.ok(),await response.text());return(await response.json()).cells;};
 // Decimal scale may retain harmless trailing zeros; compare decimal characters, never JS numbers.
 const canonical=value=>value.includes('.')?value.replace(/0+$/,'').replace(/\.$/,''):value;
 const expect=async(name,value)=>{await go(name);await cell(name).filter({hasText:value}).waitFor();assert.equal(canonical(await cell(name).innerText()),value);const raw=(await viewport())[name].raw_value;if(value.startsWith('#'))assert.equal(raw.type,'Error');else{assert.equal(raw.type,'Decimal');assert.equal(canonical(raw.value),value,'Exact decimal strings must never round-trip through JavaScript Number');}};
 const cases=[
  ['C1','=CEILING.MATH(A2,10)','10'],
  ['C2','=FLOOR.MATH(A3,10)','-10'],
  ['C3','=CEILING.MATH(A3,10)','0'],
  ['C4','=FLOOR.MATH(A2,10)','0'],
  ['C5','=CEILING.MATH(A3,10,1)','-10'],
  ['C6','=FLOOR.MATH(A3,10,1)','0'],
  ['C7','=CEILING.MATH(A2,-10)','10'],
  ['C8','=FLOOR.MATH(A3,-10)','-10'],
  ['C9','=CEILING.MATH(A3,10,-1)','-10'],
  ['C10','=FLOOR.MATH(A3,10,-1)','0'],
  ['C11','=CEILING.MATH(A1,A2)',max],
  ['C12','=FLOOR.MATH(A1,A2)',max],
  ['C13','=CEILING.MATH(A1,10)','#NUM!'],
  ['C14','=FLOOR.MATH(A1,10)','79228162514264337593543950330'],
  ['E1','=CEILING.MATH(A4)','5'],
  ['E2','=FLOOR.MATH(A4)','4'],
  ['E3','=CEILING.MATH(-A4)','-4'],
  ['E4','=FLOOR.MATH(-A4)','-5'],
  ['E5','=CEILING.MATH(-A4,1,1)','-5'],
  ['E6','=FLOOR.MATH(-A4,1,1)','-4'],
  ['E7','=CEILING.MATH(A4,0)','0'],
  ['E8','=FLOOR.MATH(A4,0)','0'],
  ['E9','=CEILING.MATH(1/0,1)','#DIV/0!'],
  ['E10','=FLOOR.MATH(1,1/0)','#DIV/0!'],
  ['E11','=CEILING.MATH(A2,A1)',max],
  ['G1','=PRODUCT(E1,2)','10'],
 ];
 await grid().waitFor();for(const[name,formula,value]of cases){await edit(name,formula);await expect(name,value);}
 // Extreme inputs return cell results/errors; subsequent edits must still recalculate.
 assert((await page.request.get(`${base}/api/workbook`)).ok());await edit('A4','5.3');await expect('E1','6');await expect('E2','5');await expect('G1','12');await expect('C13','#NUM!');
 const saved=path.join(artifacts,'ceiling-floor.tay');page.once('dialog',d=>d.accept(saved));await page.getByRole('tab',{name:'File',exact:true}).click();await page.getByRole('button',{name:'Save',exact:true}).click();await page.getByText(`Saved workbook to ${saved}`,{exact:true}).waitFor();
 await edit('A4','8.1');await expect('G1','18');await page.getByRole('tab',{name:'File',exact:true}).click();await page.getByRole('button',{name:'Open',exact:true}).click();const replacement=page.getByRole('dialog',{name:'Replace current workbook?',exact:true});page.once('dialog',d=>d.accept(saved));await replacement.getByRole('button',{name:'Discard and continue',exact:true}).click();await page.getByText(`Opened workbook from ${saved}`,{exact:true}).waitFor();await idle();
 await expect('G1','12');await expect('C1','10');await expect('C2','-10');await expect('C11',max);await expect('C13','#NUM!');await expect('C14','79228162514264337593543950330');assert((await page.request.get(`${base}/api/workbook`)).ok());
 await page.screenshot({path:path.join(artifacts,'ceiling-floor.png'),fullPage:true});assert.deepEqual(errors,[]);console.log('PASS exact CEILING.MATH/FLOOR.MATH tiny and maximum magnitudes, signed significance/modes, defaults/zero/errors, dependent recalculation and save/reopen.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
