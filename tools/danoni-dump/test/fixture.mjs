import { createServer } from 'node:http';
import { createHash } from 'node:crypto';

// Original, synthetic transport fixture. This deliberately is not a DanOni
// implementation or an oracle for DanOni semantics. No downloaded assets.
export function silentWave() {
  const samples = 800;
  const bytes = Buffer.alloc(44 + samples * 2);
  bytes.write('RIFF', 0);
  bytes.writeUInt32LE(bytes.length - 8, 4);
  bytes.write('WAVEfmt ', 8);
  bytes.writeUInt32LE(16, 16);
  bytes.writeUInt16LE(1, 20);
  bytes.writeUInt16LE(1, 22);
  bytes.writeUInt32LE(8000, 24);
  bytes.writeUInt32LE(16000, 28);
  bytes.writeUInt16LE(2, 32);
  bytes.writeUInt16LE(16, 34);
  bytes.write('data', 36);
  bytes.writeUInt32LE(samples * 2, 40);
  return bytes;
}

const inlineDos = '|musicTitle=Synthetic,DDI test,https://example.invalid/|tuning=DDI test,https://example.invalid/|difData=Tiny,First,3.5$Tiny,Second,4|musicUrl=./music.js|blankFrame=200|adjustment=1.5|playbackRate=2|left_data=210,230|right_data=240|frzLeft_data=250,270|left2_data=280|right2_data=300|frzRight2_data=320,340|speed_data=200,1,260,2|boost_data=200,1,280,0.5|color_data=220,0,#ff0000|word_data=220,0,Synthetic lyric|preloadImages=./img/pixel.svg|keyExtraList=Tiny|charaTiny=left,right|keyCtrlTiny=KeyD,KeyK|';

const engine = String.raw`'use strict';
const g_version = 'Ver 50.2.0-synthetic';
const g_revisedDate = '2026/10/08';
const g_fps = 60;
const g_enableAmpersandSplit = true;
const g_enableDecodeURI = false;
const fixtureMusicMenu = new URL(location.href).searchParams.get('menu') === '1';
const g_rootObj = {};
const g_headerObj = {
  keyLabels: ['Tiny', 'Tiny'], difLabels: ['First', 'Second'],
  creatorNames: ['DDI test', 'DDI test'], difficulties: [1, 2],
  initSpeeds: [3.5, 4], keyExtraList: ['Tiny'],
  musicTitles: ['Synthetic'], artistNames: ['DDI test'],
  musicUrls: ['./music.js', './music-second.js'], musicNos: fixtureMusicMenu ? [1, 0] : [0, 0],
  blankFrame: 200, blankFrameDef: 200, blankFrameDefs: [200, 200],
  adjustment: [1.5, 1.5], playbackRate: 2,
  viewLists: [0, 1], scoreNos: [1, 2], dosNos: [1, 1],
  musicFolder: './', tuning: 'DDI test', tuningInit: 'DDI test'
};
const g_stateObj = {
  scoreId: Number(new URL(location.href).searchParams.get('scoreId') || 0),
  adjustment: 0, shuffle: 'OFF', motion: 'OFF', reverse: 'OFF',
  fadein: 0, speed: 3.5, dummyId: '', scoreLockFlg: false,
  dosDivideFlg: false, d_velocity: 'ON', d_color: 'ON',
  d_lyrics: 'ON', d_background: 'ON', d_arroweffect: 'ON'
};
const g_keyObj = {
  currentKey: 'Tiny', currentPtn: 0, defaultProp: 'chara',
  charaTiny_0: ['left', 'right'], colorTiny_0: [0, 1],
  keyCtrlTiny_0: [[68], [75]], stepRtnTiny_0: [0, 180],
  shuffleTiny_0: [0, 0], posTiny_0: [0, 1], divTiny_0: 2,
  scrollDirTiny_0: {'---': [1, 1]}
};
const g_escapeStr = { frzName: [] };
const g_gaugeHeaderObj = [
  {Border: 70, Recovery: 2, Damage: 7, Init: 25},
  {Border: 70, Recovery: 2, Damage: 7, Init: 25}
];
const replaceStr = value => value;
const toCapitalize = value => value[0].toUpperCase() + value.slice(1);
const g_customJsObj = { preloading: [() => {
  g_rootObj.left_data = '212,232';
  g_rootObj.left2_data = '282';
}], loading: [] };
let g_externalDos = '';
let g_musicdata = '';
let g_scoreObj = {};
let g_currentPage = 'title';
const g_loadObj = {main: true};
const setScoreIdHeader = scoreId => scoreId === 0 ? '' : String(scoreId + 1);
const setDosIdHeader = () => '';
const getFrzName = name => 'frz' + name[0].toUpperCase() + name.slice(1);
const getMusicUrl = (id = g_stateObj.scoreId) => g_headerObj.musicUrls[g_headerObj.musicNos[id]];
const getFullMusicUrl = value => new URL(value, location.href).href;
const getKeyInfo = () => ({keyCtrlPtn: 'Tiny_0', keyNum: 2});
const setDifficulty = () => {
  g_keyObj.currentKey = g_headerObj.keyLabels[g_stateObj.scoreId];
  g_keyObj.currentPtn = 0;
};
const dosConvert = (_dos = '') => {
  const obj = {};
  for (const entry of _dos.split('|')) {
    const position = entry.indexOf('=');
    if (position > 0) obj[entry.slice(0, position)] = entry.slice(position + 1);
  }
  return obj;
};
const scoreConvert = (_dosObj, _scoreId, _preblankFrame, _dummyNo = '',
  _keyCtrlPtn = 'Tiny_0', _scoreAnalyzeFlg = false) => {
  const obj = {};
  const suffix = setScoreIdHeader(_scoreId);
  g_stateObj.realAdjustment = (g_stateObj.adjustment + g_headerObj.adjustment[_scoreId]) / g_headerObj.playbackRate + _preblankFrame;
  g_stateObj.intAdjustment = Math.floor(g_stateObj.realAdjustment);
  g_stateObj.decimalAdjustment = g_stateObj.realAdjustment - g_stateObj.intAdjustment;
  const frame = value => Math.round((Number(value) - 200) / 2 + 200 + g_stateObj.intAdjustment);
  const notes = name => (_dosObj[name] || '').split(',').filter(Boolean).map(frame);
  obj.arrowData = g_keyObj.charaTiny_0.map(name => notes(name + suffix + '_data'));
  obj.frzData = g_keyObj.charaTiny_0.map(name => notes(getFrzName(name) + suffix + '_data'));
  obj.dummyArrowData = [[], []];
  obj.dummyFrzData = [[], []];
  obj.speedData = [frame(200), 1, frame(260), 2];
  obj.boostData = [frame(200), 1, frame(280), 0.5];
  obj.colorData = [frame(220), 0, '#ff0000', false];
  if (_scoreAnalyzeFlg) return obj;
  obj.wordData = [[frame(220), 0, 'Synthetic lyric']];
  obj.keychFrames = [0];
  obj.keychTarget = [['0']];
  obj.keychTargetAlpha = [[1]];
  return obj;
};
const script = path => new Promise((resolve, reject) => {
  const element = document.createElement('script');
  element.src = path;
  element.onload = resolve;
  element.onerror = reject;
  document.head.append(element);
});
const loadChartFile = async () => {
  Object.assign(g_rootObj, dosConvert(document.getElementById('dos').value));
  await script('./dos.js?' + Date.now());
  externalDosInit();
  Object.assign(g_rootObj, dosConvert(g_externalDos));
};
const pushArrows = () => {};
const mainInit = () => {
  g_currentPage = 'main';
};
const loadMusic = async () => {
  if (fixtureMusicMenu && (g_rootObj.menuAccepted !== 'true' ||
      g_rootObj.menuSelection !== String(g_headerObj.musicNos[g_stateObj.scoreId]))) {
    throw new Error('The menu input and Enter hooks must select the chart music');
  }
  g_currentPage = 'loading';
  await script(getMusicUrl());
  for (const hook of g_customJsObj.preloading) hook();
  g_scoreObj = scoreConvert(g_rootObj, g_stateObj.scoreId, 0);
  g_scoreObj = scoreConvert(g_rootObj, g_stateObj.scoreId, 20);
  pushArrows(g_scoreObj);
  for (const hook of g_customJsObj.loading) hook();
  mainInit();
};
const optionInit = () => {
  g_currentPage = 'option';
  setDifficulty(true);
  document.getElementById('divRoot').innerHTML = '<button id="btnPlay">Play</button>';
  document.getElementById('btnPlay').onclick = loadMusic;
};
const musicSelectInit = () => {
  g_currentPage = 'musicselect';
  document.getElementById('divRoot').innerHTML = '<input id="lblMusicNumBarSlider" type="range" min="0" max="1" value="0">';
  const slider = document.getElementById('lblMusicNumBarSlider');
  slider.addEventListener('input', () => {
    g_rootObj.menuSelection = slider.value;
    g_stateObj.menuInputEvents = (g_stateObj.menuInputEvents || 0) + 1;
  });
  const accept = event => {
    if (event.key !== 'Enter' || g_rootObj.menuSelection === undefined) return;
    g_rootObj.menuAccepted = 'true';
    g_stateObj.menuAcceptEvents = (g_stateObj.menuAcceptEvents || 0) + 1;
    document.removeEventListener('keydown', accept);
    optionInit();
  };
  document.addEventListener('keydown', accept);
};
loadChartFile().then(() => {
  document.getElementById('btnStart').onclick = fixtureMusicMenu ? musicSelectInit : optionInit;
  document.documentElement.dataset.ready = 'true';
});
`;

/** Start an HTTP fixture; callers must await close() even after failed tests. */
export async function startFixture({ musicMenu = false, missingImage = false } = {}) {
  const requests = [];
  const routes = new Map([
    ['/robots.txt', ['text/plain', 'User-agent: *\nDisallow: /denied/\n']],
    ['/work/index.html', ['text/html; charset=utf-8', `<!doctype html><meta charset="utf-8"><title>Synthetic DanOni transport fixture</title><input type="hidden" id="dos" value="${inlineDos}"><input type="hidden" id="externalDos" value="./dos.js"><div id="divRoot"><button id="btnStart">Start</button></div><img src="./img/pixel.svg" alt="Synthetic square"><script src="./engine.js"></script>`]],
    ['/work/engine.js', ['text/javascript', engine]],
    ['/work/dos.js', ['text/javascript', "function externalDosInit() { const frames = [260, 280]; g_externalDos = '|right_data=' + frames.join(',') + '|tuning=Generated fixture,https://example.invalid/|'; }"]],
    ['/work/music.js', ['text/javascript', `g_musicdata = 'data:audio/wav;base64,${silentWave().toString('base64')}';`]],
    ['/work/music-second.js', ['text/javascript', `g_musicdata = 'data:audio/wav;base64,${silentWave().toString('base64')}';`]],
    ['/work/img/pixel.svg', ['image/svg+xml', '<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><path fill="#f00" d="M0 0h4v4H0z"/></svg>']],
  ]);
  if (missingImage) routes.delete('/work/img/pixel.svg');
  const server = createServer((request, response) => {
    const url = new URL(request.url, 'http://localhost');
    requests.push(url.pathname + url.search);
    const route = routes.get(url.pathname);
    if (!route) {
      response.writeHead(404, {'Content-Type': 'text/plain'}).end('Not found');
      return;
    }
    response.writeHead(200, {'Content-Type': route[0]}).end(route[1]);
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return {
    url: `http://127.0.0.1:${server.address().port}/work/index.html${musicMenu ? '?menu=1' : ''}`,
    engineSha256: createHash('sha256').update(engine).digest('hex'),
    requests,
    close: () => new Promise((resolve, reject) => {
      server.close(error => error ? reject(error) : resolve());
      server.closeAllConnections();
    }),
  };
}
