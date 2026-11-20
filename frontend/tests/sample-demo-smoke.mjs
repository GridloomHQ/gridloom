import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const temp=mkdtempSync(path.join(tmpdir(),'gridloom-sample-ui-'));
const server=spawn(process.env.GRIDLOOM_SERVER??path.join(root,'target/debug/gridloom-server'),['--port','0','--no-samples'],{
  cwd:root,env:{...process.env,GRIDLOOM_RECOVERY_DIR:temp},stdio:['ignore','pipe','pipe']
});
let log='',browser;
server.stdout.on('data',d=>log+=d);
server.stderr.on('data',d=>log+=d);
try{
  let base;
  const deadline=Date.now()+30000;
  while(Date.now()<deadline && server.exitCode===null && !(base=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/)?.[1]))await new Promise(r=>setTimeout(r,50));
  assert(base,log);
  browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||(existsSync('/usr/bin/chromium')?'/usr/bin/chromium':undefined),args:['--no-sandbox']});
  const page=await browser.newPage({locale:'en-US'});
  await page.goto(base);
  await page.getByRole('tab',{name:'Help',exact:true}).click();
  await page.getByText('About',{exact:true}).click();
  const about=page.getByRole('dialog',{name:'About Gridloom'});
  await about.getByRole('button',{name:'Generate 10-million-row sample'}).click();
  const browse=about.getByRole('button',{name:'Open sample worksheet'});
  await browse.waitFor({timeout:180_000});
  assert((await about.innerText()).includes('10,000,000 rows'));
  await browse.click();
  const ledger=page.getByRole('region',{name:'Sample ledger rows'});
  await ledger.getByText(/10,000,000 matching rows/).waitFor({timeout:30_000});
  await ledger.getByLabel('Go to row').fill('10000000');
  const [response]=await Promise.all([
    page.waitForResponse(r=>r.url().endsWith('/api/sample/ledger')&&r.request().postDataJSON().offset===9999900),
    ledger.getByRole('button',{name:'Go',exact:true}).click(),
  ]);
  assert(response.ok(),await response.text());
  const data=await response.json();
  assert.equal(data.rows.at(-1)[0],'10000000');
  assert(data.rows.length<=200);
  await ledger.getByLabel('Account code').fill('1100');
  await ledger.getByLabel('From',{exact:true}).fill('2026-01-01');
  await ledger.getByLabel('Through',{exact:true}).fill('2026-01-07');
  await ledger.getByRole('button',{name:'Load / reload'}).click();
  await ledger.getByText(/70,000 matching rows/).waitFor({timeout:30_000});
  const worksheet=page.getByRole('region',{name:'Sample worksheet'});
  await worksheet.getByRole('button',{name:'SQL',exact:true}).click();
  await worksheet.getByRole('button',{name:'Run query'}).click();
  const results=worksheet.getByRole('table');
  await results.getByRole('cell',{name:'10000000',exact:true}).waitFor();
  await worksheet.getByRole('button',{name:'Last entry'}).click();
  await worksheet.getByRole('button',{name:'Run query'}).click();
  await results.getByRole('columnheader',{name:'id',exact:true}).waitFor();
  await results.getByRole('cell',{name:'10000000',exact:true}).waitFor();
  await worksheet.getByLabel('SQL against the sample').fill('SELECT id FROM entries ORDER BY id');
  await worksheet.getByRole('button',{name:'Run query'}).click();
  await worksheet.getByText('Showing the first 200 result rows.').waitFor();
  assert.equal(await results.getByRole('row').count(),201);
  await worksheet.getByRole('button',{name:'Back to workbook'}).click();
  await page.getByRole('button',{name:'Sheet1',exact:true}).waitFor();
  await page.getByRole('button',{name:'Sample ledger (read-only)',exact:true}).click();
  await worksheet.getByRole('heading',{name:'Sample worksheet'}).waitFor();
  const denied=await page.request.post(base+'/api/sample/query',{data:{sql:'DELETE FROM entries'}});
  assert.equal(denied.status(),400);
  assert.deepEqual(await (await page.request.get(base+'/api/connections')).json(),[]);
  console.log('10-million-row sample browser smoke passed');
}finally{
  await browser?.close();
  server.kill();
  if(server.exitCode===null&&server.signalCode===null)await new Promise(resolve=>server.once('exit',resolve));
  rmSync(temp,{recursive:true,force:true});
}
