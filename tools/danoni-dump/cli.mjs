#!/usr/bin/env node
import { parseArgs } from 'node:util';
import path from 'node:path';
import { capture, verify } from './dump.mjs';

const help = `Usage:
  node cli.mjs capture URL --out DIRECTORY [--cache DIRECTORY] [--charts 0,1]
      [--terms-notes TEXT] [--trust-engine SHA256] [--timeout-ms 120000]
  node cli.mjs verify DIRECTORY --offline [--timeout-ms 120000]

Downloaded content must be outside Git checkouts. Capture is anonymous,
robots-aware and paced at one request/second/origin; no site script runs in Node.
--trust-engine is only for source hashes you have inspected for this adapter.
`;

try {
  const { values, positionals } = parseArgs({ allowPositionals: true, options: {
    out: { type: 'string' }, cache: { type: 'string' }, charts: { type: 'string' },
    'terms-notes': { type: 'string', default: '' }, 'trust-engine': { type: 'string', multiple: true, default: [] },
    'timeout-ms': { type: 'string', default: '120000' }, offline: { type: 'boolean' }, help: { type: 'boolean' },
  } });
  if (values.help) { console.log(help); }
  else {
    const [command, target, ...rest] = positionals;
    if (!target || rest.length || !['capture', 'verify'].includes(command)) throw new Error(help);
    const timeoutMs = Number(values['timeout-ms']);
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1000) throw new Error('Invalid timeout');
    if (values['trust-engine'].some(h => !/^[a-f0-9]{64}$/.test(h))) throw new Error('Invalid source SHA-256');
    const onProgress = message => console.error(message);
    let result;
    if (command === 'capture') {
      if (!values.out || values.offline) throw new Error(help);
      const parsedUrl = new URL(target);
      if (!['https:', 'http:'].includes(parsedUrl.protocol) || parsedUrl.username || parsedUrl.password) throw new Error('Expected anonymous HTTP(S) work URL');
      const chartIds = values.charts?.split(',').map(Number);
      if (values.charts !== undefined && values.charts.split(',').some(token => !/^\d+$/.test(token))) throw new Error('Invalid chart indices');
      if (chartIds && (!chartIds.length || chartIds.some(n => !Number.isSafeInteger(n) || n < 0) || new Set(chartIds).size !== chartIds.length)) throw new Error('Invalid chart indices');
      result = await capture({ url: target, outDir: path.resolve(values.out), cacheDir: values.cache && path.resolve(values.cache), chartIds,
        trustedHashes: values['trust-engine'], termsNotes: values['terms-notes'], timeoutMs, onProgress });
      console.log(JSON.stringify({ status: result.status, charts: result.charts.map(c => ({ chartId: c.chartId, status: c.status })), outDir: path.resolve(values.out) }, null, 2));
      if (result.charts.some(c => c.status !== 'complete')) process.exitCode = 2;
    } else {
      if (!values.offline) throw new Error('Verification requires --offline');
      result = await verify(path.resolve(target), { timeoutMs, onProgress });
      console.log(JSON.stringify(result, null, 2));
      if (!result.passed) process.exitCode = 2;
    }
  }
} catch (error) { console.error(error.message); process.exitCode = 1; }
