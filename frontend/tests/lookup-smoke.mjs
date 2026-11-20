// Real browser + local server with synthetic data. Faults/delays are explicit route interceptions.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const artifacts=path.join(root,'target/lookup-smoke');mkdirSync(artifacts,{recursive:true});
const server=spawn(process.env.GRIDLOOM_SERVER||path.join(root,'target/debug/gridloom-server'),['--port','0'],{cwd:root,env:process.env,stdio:['ignore','pipe','pipe']});
let log='',browser,page;server.stdout.on('data',d=>log+=d);server.stderr.on('data',d=>log+=d);
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
try {
 const base=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{clearInterval(poll);reject(new Error(`Server timeout: ${log}`));},30000);const poll=setInterval(()=>{const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearInterval(poll);clearTimeout(timer);resolve(m[1]);}},50);server.once('exit',code=>{clearInterval(poll);clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:existsSync('/usr/bin/chromium')?{executablePath:'/usr/bin/chromium'}:{}),args:['--no-sandbox']});
 page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const seed=async(row,col,raw_input)=>{const response=await page.request.post(`${base}/api/cells/set`,{data:{sheet_name:'Sheet1',row,col,raw_input:String(raw_input)}});assert(response.ok(),await response.text());};
 for(let i=0;i<3;i++){await seed(i,0,[10,20,30][i]);await seed(i,1,[100,200,300][i]);await seed(i+4,0,[30,10,20][i]);await seed(i+4,1,[300,100,200][i]);await seed(0,i+3,[10,20,30][i]);await seed(1,i+3,[100,200,300][i]);await seed(2,i+3,[1000,2000,3000][i]);}
 await page.goto(base);const grid=()=>page.getByRole('grid');const cell=name=>page.getByRole('gridcell',{name,exact:true});const address=page.getByRole('textbox',{name:'Go to cell',exact:true});const idle=()=>page.waitForFunction(()=>document.querySelector('[role="grid"]')?.getAttribute('aria-busy')==='false');
 const go=async name=>{await address.fill(name);await address.press('Enter');await cell(name).and(page.locator('[aria-selected="true"]')).waitFor();await page.waitForFunction(()=>!document.querySelector('input[aria-label="Formula input"]')?.disabled);};
 const edit=async(name,value)=>{await go(name);await grid().focus();await grid().press('F2');const input=page.getByRole('textbox',{name:`Edit ${name}`,exact:true});await input.fill(value);await input.press('Enter');await idle();};
 const expectCell=async(name,value)=>{await go(name);await cell(name).filter({hasText:value}).waitFor();assert.equal(await cell(name).innerText(),value);};
 const cases=[
  ['H1','=XLOOKUP(25,A1:A3,B1:B3,999,-1)','200'],
  ['H2','=XLOOKUP(20,A1:A3,B1:B3,999,-1)','200'],
  ['H3','=XLOOKUP(25,A1:A3,B1:B3,999,1)','300'],
  ['H4','=XLOOKUP(25,A5:A7,B5:B7,999,-1)','200'],
  ['H5','=XLOOKUP(25,A5:A7,B5:B7,999,1)','300'],
  ['H7','=XLOOKUP(20,D1:F1,D2:F3)','200'],
 ];
 // Separate invalid cases avoid conflating a shape error with no-match fallback.
 cases.push(
  ['J1','=XLOOKUP(20,A1:B3,B1:B3,999)','#VALUE!'],
  ['J2','=XLOOKUP(20,A1:A3,B1:B3,999,3)','#VALUE!'],
  ['J3','=XLOOKUP(20,A1:A3,B1:B3,999,0,0)','#VALUE!'],
  ['J4','=XLOOKUP(1/0,A1:A3,B1:B3,999)','#DIV/0!'],
  ['J5','=VLOOKUP(10,A1:B3,3,FALSE)','#REF!'],
 );
 await grid().waitFor();for(const [name,formula,value] of cases){await edit(name,formula);await expectCell(name,value);}
 await expectCell('H8','2000');
 const vpResponse=await page.request.post(`${base}/api/cells/viewport`,{data:{sheet_name:'Sheet1',start_row:0,start_col:7,end_row:8,end_col:9,overscan:0}});const viewport=await vpResponse.json();assert.equal(viewport.cells.H8.is_spill,true);assert.equal(viewport.cells.H7.raw_input,'=XLOOKUP(20,D1:F1,D2:F3)');
 await go('H1');await page.getByRole('tab',{name:'Review',exact:true}).click();await page.getByRole('button',{name:'Explain & Review',exact:true}).click();const explain=page.getByRole('complementary',{name:'Explain and review selected cell'});await explain.getByRole('heading',{name:'Current calculated value',exact:true}).waitFor();await explain.getByText('200',{exact:true}).first().waitFor();await explain.locator('section').filter({has:page.getByRole('heading',{name:'Current calculated value',exact:true})}).getByText('=XLOOKUP(25,A1:A3,B1:B3,999,-1)',{exact:true}).waitFor();await explain.getByRole('button',{name:'Close explanation'}).click();
 const saved=path.join(artifacts,'lookup.gridloom');page.once('dialog',d=>d.accept(saved));await page.getByRole('tab',{name:'File',exact:true}).click();await page.getByRole('button',{name:'Save',exact:true}).click();await page.getByText(`Saved workbook to ${saved}`,{exact:true}).waitFor();
 await edit('B2','999');await expectCell('H1','999');
 await page.getByRole('tab',{name:'File',exact:true}).click();await page.getByRole('button',{name:'Open',exact:true}).click();const replacement=page.getByRole('dialog',{name:'Replace current workbook?',exact:true});page.once('dialog',d=>d.accept(saved));await replacement.getByRole('button',{name:'Discard and continue',exact:true}).click();await page.getByText(`Opened workbook from ${saved}`,{exact:true}).waitFor();await idle();await expectCell('H1','200');await expectCell('H8','2000');await expectCell('J4','#DIV/0!');
 await edit('B2','240');await expectCell('H1','240');await expectCell('H2','240');
 await page.screenshot({path:path.join(artifacts,'lookup.png'),fullPage:true});assert.deepEqual(errors,[]);console.log('PASS XLOOKUP nearest/exact/unsorted matches, horizontal multirow spill, invalid modes/shapes, propagated errors, VLOOKUP column bounds, explanation and save/reopen/recalculation.');
}catch(error){writeFileSync(path.join(artifacts,'server.log'),log);if(page)await page.screenshot({path:path.join(artifacts,'failure.png'),fullPage:true}).catch(()=>{});throw error;}
finally{if(browser)await browser.close();server.kill('SIGTERM');}
