// Runtime imported-chart CPU checks against the native motor and production judge.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync,existsSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {execFileSync} from 'node:child_process';
const {chromium,firefox}=await import(pathToFileURL(resolve(process.env.DDI_PLAYWRIGHT||'target/scratch/cpu-web/node_modules/playwright-core/index.mjs')));
const {launchFirefox}=await import(pathToFileURL(resolve('.claude/skills/verify/firefox.mjs')));
const base=process.env.DDI_CPU_WEB_URL||'http://127.0.0.1:18878/';
const out=resolve(process.env.DDI_CPU_WEB_OUTPUT||'target/scratch/cpu-automatic/browser');
assert(!existsSync(resolve(out,'../completion.json')), 'Choose a new DDI_CPU_WEB_OUTPUT; the parent run is sealed.');
mkdirSync(out,{recursive:true});
const cases=JSON.parse(readFileSync('target/scratch/cpu-automatic/cases.json'));
const records=[];
async function run(browser,name) {
 const context=await browser.newContext({viewport:{width:1100,height:740}});
 await context.addInitScript(()=>{
  delete window.showDirectoryPicker;
  localStorage.setItem('ddi.settings.v1',JSON.stringify({volume:0,video:'Off',noteskin:'builtin'}));
  let offset=0;const now=performance.now.bind(performance);
  Object.defineProperty(performance,'now',{value:()=>now()+offset*1000});
  const raf=window.requestAnimationFrame.bind(window);window.requestAnimationFrame=fn=>raf(t=>fn(t+offset*1000));
  const get=Object.getOwnPropertyDescriptor(BaseAudioContext.prototype,'currentTime').get;
  Object.defineProperty(BaseAudioContext.prototype,'currentTime',{get(){return get.call(this)+offset;}});
  AudioContext.prototype.getOutputTimestamp=function(){return {contextTime:this.currentTime-(this.baseLatency||0)-(this.outputLatency||0),performanceTime:performance.now()};};
  window.__cpuJump=s=>{offset+=s;};
 });
 await context.addInitScript({path:resolve('.claude/skills/verify/mute-audio.js')});
 const page=await context.newPage();page.setDefaultTimeout(60000);
 const errors=[],fetches=[];page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.url().includes('/cpu/'))fetches.push(r.url());});
 await page.goto(`${base}#/select${name==='firefox'?'?gfx=gl':''}`);
 await page.locator('.import-panel input[type=file]').nth(1).setInputFiles(resolve('target/scratch/cpu-automatic/pack.zip'));
 await page.waitForFunction(count=>window.__DDI_DEBUG?.songs?.imported===count && !window.__DDI_DEBUG.songs.importing, cases.length);
 const songs=await page.evaluate(()=>new Promise((ok,no)=>{const r=indexedDB.open('ddi-library');r.onerror=()=>no(r.error);r.onsuccess=()=>{const db=r.result;const q=db.transaction('songs').objectStore('songs').getAll();q.onerror=()=>no(q.error);q.onsuccess=()=>{db.close();ok(q.result.map(x=>JSON.parse(x).entry));};};}));
 writeFileSync(`${out}/${name}-import.json`,JSON.stringify(songs,null,2));
 async function play(entry,c,strength,seed,skills="balanced") {
  const start=performance.now();
  await page.goto(`${base}#/play?song=${entry.id}&chart=0&cpu=1&strength=${strength}&seed=${seed}&skills=${encodeURIComponent(skills)}${name==='firefox'?'&gfx=gl':''}`);
  await page.waitForFunction(()=>window.__DDI_DEBUG?.prompt?.step==='ready' && window.__DDI_DEBUG?.ui?.revealed && !window.__DDI_DEBUG.ui.holding);
  await page.waitForTimeout(600);
  const readyMs=performance.now()-start;
  if(skills==='1.1,0.9,1,1.25,0.8,1,1') await page.screenshot({path:`${out}/${name}-personality-prompt.png`});
  if(skills.startsWith('1.111,')) {
   await page.setViewportSize({width:390,height:844});await page.waitForTimeout(300);
   await page.screenshot({path:`${out}/${name}-personality-mobile.png`});
   await page.setViewportSize({width:1100,height:740});await page.waitForTimeout(300);
  }
  await page.evaluate(()=>{delete window.__DDI_CPU_PLAY;delete window.__DDI_DEBUG.song_time;});
  await page.keyboard.press('Enter');
  await page.waitForFunction(()=>window.__DDI_DEBUG?.song_time!==undefined);
  if(c.title==='CPU Mixed' && strength==='normal') {await page.evaluate(()=>window.__cpuJump(4));await page.waitForTimeout(100);await page.screenshot({path:`${out}/${name}-playing.png`});}
  await page.evaluate(s=>window.__cpuJump(s+6),c.duration);
  await page.waitForFunction(()=>window.__DDI_CPU_PLAY!==undefined);
  await page.waitForFunction(()=>window.__DDI_DEBUG?.results);
  const actual=await page.evaluate(()=>({play:window.__DDI_CPU_PLAY,history:window.__DDI_DEBUG.history}));
  const expected=JSON.parse(execFileSync(resolve('target/debug/examples/cpu_chart'),['automatic',c.path,'0',strength,String(seed),skills],{encoding:'utf8',maxBuffer:32*1024*1024}));
  assert.deepEqual(actual.play.log,expected.log,`${name}/${c.title}/${strength}: log`);
  for(const k of ['score','tally','timeline'])assert.deepEqual(actual.play.results[k],expected.results[k],`${name}/${c.title}/${strength}: ${k}`);
  assert.equal(actual.play.skills_version,1);
  if(skills.includes(','))assert.deepEqual(actual.play.skills,skills.split(',').map(Number));
  assert.equal(actual.play.model,'analytical-runtime-v1');assert.equal(actual.play.results.late_edges,0);assert.equal(actual.history.count,0);
  assert.equal(fetches.length,0,'automatic mode must not request a prepared profile');
  if(skills==='jacks') {
   await page.waitForTimeout(700);
   await page.keyboard.press('r');
   await page.waitForFunction(()=>window.__DDI_DEBUG?.prompt?.step==='ready');
   await page.waitForTimeout(600);
   await page.evaluate(()=>{delete window.__DDI_CPU_PLAY;delete window.__DDI_DEBUG.song_time;});
   await page.keyboard.press('Enter');
   await page.waitForFunction(()=>window.__DDI_DEBUG?.song_time!==undefined);
   await page.evaluate(s=>window.__cpuJump(s+6),c.duration);
   await page.waitForFunction(()=>window.__DDI_CPU_PLAY!==undefined);
   const retry=await page.evaluate(()=>window.__DDI_CPU_PLAY);
   assert.deepEqual(retry.log,actual.play.log,'retry keeps personality and seed');
   assert.deepEqual(retry.skills,actual.play.skills);
  }
  const rec={browser:name,title:c.title,strength,skills,seed:String(seed),inputs:actual.play.log.inputs.length,score:actual.play.results.score.percent,readyMs,nativeExact:true,profileFetches:0};records.push(rec);console.log(rec);writeFileSync(`${out}/checks.json`,JSON.stringify(records,null,2));
 }
 try {
  for(const c of cases) {
   const entry=songs.find(s=>s.title===c.title);assert(entry);
   if(c.title==='CPU NotFour') {
    await page.goto(`${base}#/play?song=${entry.id}&chart=0&cpu=1`);
    await page.waitForFunction(()=>window.__DDI_DEBUG?.prompt?.step==='error');
    records.push({browser:name,title:c.title,unsupportedRefused:true});continue;
   }
   const levels=name==='chrome'&&c.title==='CPU Mixed'?['relaxed','easy','normal','strong','expert']:['normal'];
   for(const strength of levels) await play(entry,c,strength,strength==='expert'?'18446744073709551615':7);
  }
  const c=cases.find(c=>c.title==='CPU Mixed'),entry=songs.find(s=>s.title===c.title);
  for(const skills of ['jacks','stream','chords','stamina','technical','1.1,0.9,1,1.25,0.8,1,1','1.111,1.222,1.333,1.444,1.555,1.666,1.777']) await play(entry,c,'normal',7,skills);
  const long=cases.find(c=>c.title==='CPU Long');await play(songs.find(s=>s.title===long.title),long,'normal',7,'2,2,2,2,2,2,2');
  for(const [params,message] of [['skills=jacks','require cpu'],['cpu=1&skills=1,2','CPU skills:'],['cpu=cpu/custom.json&skills=jacks','require automatic mode']]) {
   await page.goto(`${base}#/play?song=${entry.id}&${params}`);
   await page.getByText(message,{exact:false}).first().waitFor();
  }
  assert.deepEqual(errors,[]);
 } finally {await context.close();}
}
const chrome=await chromium.launch({channel:'chrome',headless:true,args:['--enable-unsafe-webgpu','--use-angle=metal','--autoplay-policy=document-user-activation-required','--mute-audio']});
try {await run(chrome,'chrome');} finally {await chrome.close();}
const ff=await launchFirefox(firefox);
try {await run(ff,'firefox');} finally {await ff.close();}
writeFileSync(`${out}/checks.json`,JSON.stringify(records,null,2));
