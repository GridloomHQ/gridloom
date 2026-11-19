// Real browser and server; all workbook and recovery files are isolated in a temporary directory.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {existsSync,mkdtempSync,rmSync,readdirSync} from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const recovery=mkdtempSync(path.join(os.tmpdir(),'gridloom-recovery-smoke-'));
const binary=process.env.GRIDLOOM_SERVER || path.join(root,'target/debug/gridloom-server');
let server,browser;
async function start(){
  server=spawn(binary,['--port','0','--no-samples'],{cwd:root,env:{...process.env,GRIDLOOM_RECOVERY_DIR:recovery}});
  return new Promise((resolve,reject)=>{
    let log='';
    const timer=setTimeout(()=>reject(new Error(log||'Server startup timeout')),15000);
    const consume=data=>{
      log+=data;
      const m=log.match(/Server running at (http:\/\/127\.0\.0\.1:\d+)/);
      if(m){clearTimeout(timer);resolve(m[1]);}
    };
    server.stdout.on('data',consume);server.stderr.on('data',consume);
    server.once('error',error=>{clearTimeout(timer);reject(error);});
    server.once('exit',code=>{clearTimeout(timer);reject(new Error(`Server exited ${code}: ${log}`));});
  });
}
async function stop(signal='SIGTERM'){
  if(!server||server.exitCode!==null||server.signalCode!==null)return;
  const exited=new Promise(resolve=>server.once('exit',resolve));
  server.kill(signal);await exited;
}
try{
  let base=await start();
  browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||(existsSync('/usr/bin/chromium')?'/usr/bin/chromium':undefined),args:['--no-sandbox']});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(base);
  const response=await page.request.post(`${base}/api/ranges/edit`,{data:{sheet_name:'Sheet1',row:0,col:0,values:[['recover me']]}});
  assert(response.ok(),await response.text());
  for(let attempt=0;attempt<100&&!readdirSync(recovery).some(name=>name.endsWith('.gridloom'));attempt++)await new Promise(resolve=>setTimeout(resolve,50));
  assert.equal(readdirSync(recovery).filter(name=>name.endsWith('.gridloom')).length,1);
  await stop('SIGKILL');
  base=await start();
  await page.goto(base);
  const confirmation=page.getByRole('dialog',{name:'Replace current workbook?',exact:true});
  await page.getByRole('button',{name:'Recover autosave',exact:true}).click();
  await confirmation.getByRole('button',{name:'Cancel',exact:true}).click();
  await confirmation.waitFor({state:'hidden'});
  assert.equal(await page.getByRole('gridcell',{name:'A1',exact:true}).innerText(),'');
  await page.getByRole('button',{name:'Recover autosave',exact:true}).click();
  await confirmation.getByRole('button',{name:'Discard and continue',exact:true}).click();
  await page.getByRole('gridcell',{name:'A1',exact:true}).filter({hasText:'recover me'}).waitFor();
  assert.equal((await (await page.request.get(`${base}/api/workbook`)).json()).file_path,null);
  assert.deepEqual(errors,[]);
  console.log('Autosaved an untitled workbook, killed the process, restarted, cancelled recovery once, and restored via the UI.');
}finally{
  await browser?.close();
  await stop();
  rmSync(recovery,{recursive:true,force:true});
}
