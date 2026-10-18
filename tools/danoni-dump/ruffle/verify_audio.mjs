// Compare original and derived audio origins in installed Chrome; no playback.
import { chromium } from 'playwright-core';
import { readFile, writeFile, realpath } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { requireExternalDirectory } from '../dump.mjs';
const hash = data => createHash('sha256').update(data).digest('hex');
const [source, pack, destination] = process.argv.slice(2).map(p => path.resolve(p));
if (!source || !pack || !destination) throw new Error('Usage: verify_audio.mjs SOURCE_EXPORT DERIVED_PACK REPORT');
const manifest = JSON.parse(await readFile(path.join(pack,'manifest.json')));
const original = JSON.parse(await readFile(path.join(source,'manifest.json')));
await requireExternalDirectory(path.dirname(destination));
async function asset(root, relative, expected) {
  const actual = await realpath(path.join(root,relative));
  if (!actual.startsWith(`${root}${path.sep}`)) throw new Error('Path escapes source');
  const bytes = await readFile(actual);
  if (hash(bytes) !== expected) throw new Error('Changed music asset');
  return bytes.toString('base64');
}
const browser = await chromium.launch({channel:'chrome',headless:true});
const context = await browser.newContext();
await context.route('**/*', route => route.abort('blockedbyclient'));
await context.routeWebSocket('**/*', socket => socket.close());
const page = await context.newPage();
const comparisons = [];
try {
  for (const song of manifest.songs) {
    const sourceSha = song.sourceAudioSha256 ?? song.sourceWavSha256;
    const music = original.charts.map(c => c.music ?? original.music).find(m => m?.sha256 === sourceSha);
    if (!music) throw new Error('Source music is absent');
    const [wav, compressed] = await Promise.all([
      asset(source,music.path,music.sha256),asset(pack,song.music,song.musicSha256),
    ]);
    const comparison = await page.evaluate(async ({wav,compressed,sampleRate}) => {
      const audio = new AudioContext({sampleRate});
      const buffer = b64 => Uint8Array.from(atob(b64),c=>c.charCodeAt(0)).buffer;
      try {
        const a = await audio.decodeAudioData(buffer(wav));
        const b = await audio.decodeAudioData(buffer(compressed));
        const rate = a.sampleRate, count = Math.min(a.length,b.length), guard = 32;
        const length = Math.min(rate, Math.floor((count-2*guard)/3));
        const windows = [];
        if (length>0) for (let channel=0;channel<Math.min(a.numberOfChannels,b.numberOfChannels);channel++) {
          const x=a.getChannelData(channel), y=b.getChannelData(channel);
          for (let part=0;part<3;part++) {
            // Select an energetic window within each third, including short tracks.
            const low=Math.max(guard,Math.floor(part*count/3));
            const high=Math.min(count-length-guard,Math.floor((part+1)*count/3)-length);
            let start=low,energy=-1;
            for (let at=low;at<=high;at+=Math.max(1,Math.floor(length/2))) {
              let rms=0;
              for (let i=0;i<length;i+=16) rms+=x[at+i]**2;
              if (rms>energy) {energy=rms;start=at;}
            }
            if (energy<=1e-10) {
              windows.push({channel,part,status:'insufficient-energy'});continue;
            }
            let best = {lagSamples:null,correlation:-Infinity};
            for (let lag=-guard;lag<=guard;lag++) {
              let xy=0,xx=0,yy=0;
              for (let i=0;i<length;i++) {
                const u=x[start+i],v=y[start+i+lag];xy+=u*v;xx+=u*u;yy+=v*v;
              }
              const correlation = xy/Math.sqrt(xx*yy);
              if (correlation>best.correlation) best={lagSamples:lag,correlation};
            }
            windows.push({channel,part,status:'compared',startSeconds:start/rate,lengthSeconds:length/rate,...best});
          }
        }
        const compared=windows.filter(w=>w.status==='compared');
        return {sourceFrames:a.length,decodedFrames:b.length,sampleRate:rate,channels:b.numberOfChannels,windows,
          passed:a.length===b.length && a.sampleRate===b.sampleRate && a.numberOfChannels===b.numberOfChannels &&
            compared.length>0 && compared.every(w=>Math.abs(w.lagSamples)<=1 && w.correlation>=.97)};
      } finally {await audio.close();}
    },{wav,compressed,sampleRate:song.pcmSampleRate ?? 22050});
    comparisons.push({song:song.id,sourceAudioSha256:sourceSha,musicSha256:song.musicSha256,...comparison});
  }
  const result = {schema:'ddi-audio-origin-comparison/experimental-1',browserVersion:browser.version(),
    passed:comparisons.every(c=>c.passed),scope:'Decoded sample count and correlation in energetic windows from each third of each channel; quiet windows explicitly unverified. No source Flash timing or device latency claim',comparisons};
  await writeFile(destination,`${JSON.stringify(result,null,2)}\n`,{flag:'wx'});
  console.log(JSON.stringify({passed:result.passed,comparisons}));
  if (!result.passed) process.exitCode=2;
} finally {await browser.close();}
