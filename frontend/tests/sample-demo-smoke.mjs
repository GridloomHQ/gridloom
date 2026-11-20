import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const temp=mkdtempSync(path.join(process.env.GRIDLOOM_TEST_TMPDIR??tmpdir(),'gridloom-demo-invoices-'));
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
  const generationStart=performance.now();
  await generate.click();
  await about.getByText('10,000,000 invoices',{exact:true}).waitFor({timeout:240_000});
  console.log(`Generate 10M invoices: ${Math.round(performance.now()-generationStart)} ms including UI`);
  assert.match(await about.innerText(),/9,999,990 added/);
  assert.deepEqual(await query('SELECT * FROM invoices ORDER BY id LIMIT 10'),original);
  const count=await query('SELECT count(*), max(id) FROM invoices');
  assert.deepEqual(count[0].map(cell=>Number(cell.value)),[10_000_000,10_000_000]);
  await generate.click();
  await about.getByText(/database\. 0 added\./).waitFor({timeout:20_000});
  const summary=await (await page.request.get(base+'/api/workbook')).json();
  assert.equal(summary.sheets.length,1,'generation should not add a workbook sheet');
  assert.equal(summary.sheets[0].tables.find(t=>t.name==='Invoices').range.end_row,11);
  await about.getByRole('button',{name:'Close',exact:true}).click();
  await page.getByRole('tab',{name:'Data',exact:true}).click();
  const refresh=page.getByRole('button',{name:'Refresh',exact:true});
  const refreshStart=performance.now();
  const [refreshed]=await Promise.all([
    page.waitForResponse(response=>new URL(response.url()).pathname==='/api/tables/refresh-live'),
    refresh.click(),
  ]);
  assert(refreshed.ok(),await refreshed.text());
  const table=await refreshed.json();
  assert.equal(table.live.sql,'SELECT * FROM invoices');
  assert.equal(table.range.end_row,10_000_001);
  const update=await (await page.request.get(base+'/api/workbook')).json();
  assert.equal(update.sheets[0].tables.filter(t=>t.name==='Invoices').length,1,'refresh expands the original table');
  const grid=page.getByRole('grid',{name:'Sheet1 worksheet',exact:true});
  const first=grid.getByRole('gridcell',{name:'B3',exact:true});
  await first.getByText('1',{exact:true}).waitFor();
  await first.click();
  const windowRequests=[];
  const watchWindow=req=>{if(new URL(req.url()).pathname==='/api/cells/viewport-panes')windowRequests.push(req.postDataJSON());};
  page.on('request',watchWindow);
  console.log(`Refresh the existing worksheet table: ${Math.round(performance.now()-refreshStart)} ms to visible first invoice`);
  async function assertVisibleSelection(cell,text){
    await cell.getByText(text,{exact:true}).waitFor();
    assert.equal(await cell.getAttribute('aria-selected'),'true');
    assert.equal(await cell.evaluate(el=>{
      const c=el.getBoundingClientRect(),v=el.closest('[role="grid"]').getBoundingClientRect();
      if(c.top<v.top || c.bottom>v.bottom+1 || c.left<v.left || c.right>v.right+1)return false;
      return document.elementFromPoint((c.left+c.right)/2,(c.top+c.bottom)/2)?.closest('[role="gridcell"]')===el;
    }),true,'selected cell must be visible and uncovered at its center');
  }
  const jumpStart=performance.now();
  await grid.press('Control+ArrowDown');
  const final=grid.getByRole('gridcell',{name:'B10000002',exact:true});
  await assertVisibleSelection(final,'10000000');
  console.log(`Ctrl+Down to last invoice: ${Math.round(performance.now()-jumpStart)} ms including render`);
  assert(windowRequests.some(({start_row,end_row})=>start_row>9_999_000 && end_row-start_row<1000));
  assert(!windowRequests.some(({start_row})=>start_row>1000 && start_row<9_999_000),'jump should not fetch intermediate windows');
  page.off('request',watchWindow);
  // A full live-table refresh must invalidate cached cell values even if the
  // result has the same dimensions. Modify only this test server's owned DB.
  const finalCustomer=grid.getByRole('gridcell',{name:'C10000002',exact:true});
  const customerBefore=(await finalCustomer.innerText()).trim();
  assert(customerBefore,'the final invoice customer must already be rendered');
  const newCustomer='Fixture customer after refresh';
  assert.notEqual(customerBefore,newCustomer);
  const dbPath=path.join(temp,'fixtures','sample_sales.sqlite');
  assert(existsSync(dbPath),'the fixture must have been created under the test directory');
  const edit=spawnSync('python3',['-c',`
import sqlite3
import sys
from pathlib import Path
uri = Path(sys.argv[1]).as_uri() + "?mode=rw"
db = sqlite3.connect(uri, uri=True, timeout=10)
try:
    assert db.execute("PRAGMA application_id").fetchone()[0] == 0x474C494E
    with db:
        changed = db.execute("UPDATE invoices SET customer_name = ? WHERE id = ?", (sys.argv[2], 10_000_000))
        assert changed.rowcount == 1
finally:
    db.close()
`,dbPath,newCustomer],{encoding:'utf8',timeout:15_000});
  assert.ifError(edit.error);
  assert.equal(edit.status,0,edit.stderr);
  assert.deepEqual((await query('SELECT count(*), max(id) FROM invoices'))[0].map(cell=>Number(cell.value)),[10_000_000,10_000_000]);
  const [refreshedSameSize]=await Promise.all([
    page.waitForResponse(response=>new URL(response.url()).pathname==='/api/tables/refresh-live'),
    refresh.click(),
  ]);
  assert(refreshedSameSize.ok(),await refreshedSameSize.text());
  const sameRange=await refreshedSameSize.json();
  assert.deepEqual(sameRange.range,table.range);
  assert.equal(sameRange.name,'Invoices');
  await finalCustomer.getByText(newCustomer,{exact:true}).waitFor();
  await final.getByText('10000000',{exact:true}).waitFor();
  assert.equal(await final.getAttribute('aria-selected'),'true','refresh keeps the selected final invoice');
  // Jump back across the same logical range, then revisit the last row. The
  // round trip covers the opposite rebase and previously cached destination.
  await grid.press('Control+ArrowUp');
  await assertVisibleSelection(grid.getByRole('gridcell',{name:'B2',exact:true}),'id');
  await grid.press('Control+ArrowDown');
  await assertVisibleSelection(final,'10000000');
  await finalCustomer.getByText(newCustomer,{exact:true}).waitFor();
  await page.getByRole('button',{name:'SQL Query',exact:true}).click();
  const editor=page.getByRole('dialog',{name:'SQL Query Editor & Source Builder'});
  await editor.getByRole('combobox',{name:'Target Database:'}).selectOption('SalesDB');
  const sqlInput=editor.getByRole('textbox',{name:'SQL query'});
  const run=editor.getByRole('button',{name:'Run Query',exact:true});
  const paging=editor.getByRole('group',{name:'Query result pages'});
  const next=paging.getByRole('button',{name:'Next',exact:true});
  const previous=paging.getByRole('button',{name:'Previous',exact:true});
  const rows=editor.getByRole('table').locator('tbody > tr');
  const legacy=[];
  const observe=request=>{ if(new URL(request.url()).pathname==='/api/query/execute') legacy.push(request); };
  page.on('request',observe);
  async function assertPreview(action,{sql,offset,status,first,last,length,more}){
    const started=performance.now();
    const [response]=await Promise.all([
      page.waitForResponse(r=>new URL(r.url()).pathname==='/api/query/preview'), action.click(),
    ]);
    assert(response.ok(),await response.text());
    assert.deepEqual(response.request().postDataJSON(),{connection_name:'SalesDB',sql,offset,limit:100});
    const payload=await response.json();
    assert.equal(payload.offset,offset);
    assert.equal(payload.rows.length,length);
    assert.equal(payload.has_more,more);
    await editor.getByRole('status').filter({hasText:status}).waitFor();
    assert.equal(await rows.count(),length);
    assert.equal((await rows.first().getByRole('cell').first().innerText()).trim(),String(first));
    assert.equal((await rows.last().getByRole('cell').first().innerText()).trim(),String(last));
    assert.equal(await next.isEnabled(),more);
    assert.equal(await previous.isEnabled(),offset>0);
    console.log(`SQL preview ${status}: ${Math.round(performance.now()-started)} ms including render`);
  }
  // The original query has no LIMIT. The SQLite invoice fixture's table scan
  // returns its integer primary keys in storage order; user-defined ordering
  // is covered separately below.
  const originalSql='SELECT * FROM invoices';
  await sqlInput.fill(originalSql);
  await assertPreview(run,{sql:originalSql,offset:0,status:'Rows 1–100',first:1,last:100,length:100,more:true});
  await assertPreview(next,{sql:originalSql,offset:100,status:'Rows 101–200',first:101,last:200,length:100,more:true});
  const tailSql='SELECT * FROM invoices WHERE id >= 9999997 ORDER BY id DESC';
  await sqlInput.fill(tailSql);
  await assertPreview(run,{sql:tailSql,offset:0,status:'Rows 1–4',first:10_000_000,last:9_999_997,length:4,more:false});
  assert.deepEqual(await rows.locator('td:first-child').allTextContents(),['10000000','9999999','9999998','9999997']);
  assert.equal(legacy.length,0,'SQL editor preview must not use the eager query endpoint');
  page.off('request',observe);
  await editor.getByRole('button',{name:'Close query editor'}).click();
  await stop();
  base=await start();
  const reopened=await (await page.request.get(base+'/api/workbook')).json();
  assert.equal(reopened.sheets[0].tables.find(t=>t.name==='Invoices').range.end_row,10_000_001,'restart reattaches the live source without importing rows');
  assert.equal(Number((await query('SELECT count(*) FROM invoices'))[0][0].value),10_000_000);
  console.log('PASS: the sample button adds and preserves 10,000,000 invoices in SalesDB, refreshes the original live table, and jumps directly to the rendered final row, and reloads a changed value even at the same row count; SQL preview remains bounded.');
}finally{
  await browser?.close();
  await stop();
  rmSync(temp,{recursive:true,force:true});
}
