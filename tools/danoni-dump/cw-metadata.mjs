// Project already serialized browser state; never execute a work's scripts.
import {readFile} from 'node:fs/promises';
import {decodeSnapshot} from './serialization.mjs';
const [snapshotsFile,referenceFile]=process.argv.slice(2);
const reference=JSON.parse(await readFile(referenceFile));
const snapshots=JSON.parse(await readFile(snapshotsFile)).snapshots.map(decodeSnapshot);
const matching=snapshots.filter(s=>s.chartId===reference.chartId&&s.invocationId===reference.invocationId&&s.stage===reference.captureStage);
if(matching.length!==1)throw Error('Need one exact observed CW invocation');
const s=matching[0];
console.log(JSON.stringify({title:s.header.musicTitle,artist:s.header.artistName,creator:s.header.creatorNames?.[reference.chartId]??s.header.tuning,
  baseSpeed:s.header.baseSpeed,velocity:s.state.d_velocity??null,initialSpeed:s.header.initSpeeds[reference.chartId],selectedSpeed:s.state.speed,
  headerAdjustment:s.header.adjustment,playbackRate:s.header.playbackRate,blankFrame:s.header.blankFrame,
  referenceBpm:s.header.bpms?.[s.header.musicNos?.[reference.chartId]??0]??null}));
