import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const temp=mkdtempSync(path.join(tmpdir(),'gridloom-demo-invoices-'));
const binary=process.env.GRIDLOOM_SERVER??path.join(root,'target/debug/gridloom-server');
const args=['--port','0'];
const options={cwd:temp,env:{...process.env,GRIDLOOM_RECOVERY_DIR:path.join(temp,'recovery'),GRIDLOOM_FRONTEND_DIR:path.join(root,'frontend/dist')},stdio:['ignore','pipe','pipe']};
let server,browser;
async function start(){
  server=spawn(binary,args,options);
  let log='',base;
  server.stdout.on('data',d=>log+=d);
  server.stderr.on('data',d=>log+=d);
  const deadline=Date.now()+25_000;
  while(Date.now()<deadline && server.exitCode===null && !(base=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/)?.[1]))await new Promise(r=>setTimeout(r,50));
  assert(base,log);
  return base;
}
async function stop(){
  if(server?.exitCode===null && server.signalCode===null && server.pid){
    const exited=new Promise(resolve=>server.once('exit',resolve));
    if(server.kill())await exited;
  }
}
try{
  let base=await start();
  browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||(existsSync('/usr/bin/chromium')?'/usr/bin/chromium':undefined),args:['--no-sandbox']});
  const page=await browser.newPage({locale:'en-US'});
  async function query(sql){
    const response=await page.request.post(base+'/api/query/execute',{data:{connection_name:'SalesDB',sql}});
    assert(response.ok(),await response.text());
    return (await response.json()).rows;
  }
  const original=await query('SELECT * FROM invoices ORDER BY id LIMIT 10');
  assert.equal(original.length,10);
  await page.goto(base);
  await page.getByRole('tab',{name:'Help',exact:true}).click();
  await page.getByRole('button',{name:'About',exact:true}).click();
  const about=page.getByRole('dialog',{name:'About Gridloom'});
  const generate=about.getByRole('button',{name:'Generate demo invoices'});
  await generate.click();
  await about.getByText('10,000,000 invoices',{exact:true}).waitFor({timeout:240_000});
  assert.match(await about.innerText(),/9,999,990 added/);
  assert.deepEqual(await query('SELECT * FROM invoices ORDER BY id LIMIT 10'),original);
  const count=await query('SELECT count(*), max(id) FROM invoices');
  assert.deepEqual(count[0].map(cell=>Number(cell.value)),[10_000_000,10_000_000]);
  await generate.click();
  await about.getByText(/database\. 0 added\./).waitFor({timeout:20_000});
  const summary=await (await page.request.get(base+'/api/workbook')).json();
  assert.equal(summary.sheets.length,1,'generation should not add a workbook sheet');
  assert.equal(summary.sheets[0].tables.find(t=>t.name==='Invoices').range.end_row,11);
  await stop();
  base=await start();
  const reopened=await (await page.request.get(base+'/api/workbook')).json();
  assert.equal(reopened.sheets[0].tables.find(t=>t.name==='Invoices').range.end_row,11,'restart should not load all invoices into the worksheet');
  assert.equal(Number((await query('SELECT count(*) FROM invoices'))[0][0].value),10_000_000);
  console.log('PASS: the sample button adds and preserves 10,000,000 invoices in SalesDB, without a new worksheet or eager import.');
}finally{
  await browser?.close();
  await stop();
  rmSync(temp,{recursive:true,force:true});
}
