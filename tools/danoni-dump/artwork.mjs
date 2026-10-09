// Local typographic artwork study; original titles/credits are data, never code.
import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import { requireExternalDirectory } from './dump.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const escape = value => String(value).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;' })[c]);
const font = '"Hiragino Sans", "Noto Sans CJK JP", "Noto Sans JP", sans-serif';
const formats = { banner: [640, 200], cover: [768, 768], pack: [836, 328] };

function svg(title, artist, seed, kind, lines, size) {
  const [w, h] = formats[kind], square = kind === 'cover', hue = parseInt(seed.slice(0, 4), 16) % 360;
  const pad = square ? 55 : 32, baseline = square ? 505 : h * .48;
  const ornament = Array.from({ length: 5 }, (_, i) => {
    const x = square ? 78 + i * 147 : w - 174 + i * 32;
    const y = square ? 100 + parseInt(seed.slice(4 + i * 2, 6 + i * 2), 16) * .8 : 26 + i * 9;
    return `<path d="M${x} ${y}v${square ? 205 : 65}" stroke="hsl(${hue} 70% 74%)" stroke-opacity=".28" stroke-width="2"/><path d="M${x} ${y + 25}l8 8-8 8-8-8z" fill="hsl(${hue} 80% 78%)" fill-opacity=".6"/>`;
  }).join('');
  const text = lines.map((line, i) => `<text x="${pad}" y="${baseline + i * size * 1.2}" font-size="${size}" font-weight="700">${escape(line)}</text>`).join('');
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" role="img" aria-label="${escape(title)}">
<defs><linearGradient id="g" x2="1" y2="1"><stop stop-color="hsl(${hue} 45% 20%)"/><stop offset="1" stop-color="#10151f"/></linearGradient></defs>
<rect width="${w}" height="${h}" fill="url(#g)"/><rect x="0" y="${h - 6}" width="${w}" height="6" fill="hsl(${hue} 75% 68%)"/>${ornament}
<g fill="#f6f5ef" font-family="${escape(font)}">${text}<text x="${pad}" y="${square ? 705 : h - 27}" font-size="${square ? 24 : 18}" fill="#c8cbd0">${escape(artist)}</text></g></svg>`;
}

async function layout(page, title, artist, kind) {
  return page.evaluate(({ title, artist, kind, formats, font }) => {
    const [w, h] = formats[kind], square = kind === 'cover', canvas = document.createElement('canvas'), ctx = canvas.getContext('2d');
    const width = w - (square ? 110 : 64), maxSize = square ? 64 : kind === 'pack' ? 44 : 36;
    let chosen;
    for (let size = maxSize; size >= 16; size--) {
      ctx.font = `700 ${size}px ${font}`;
      const lines = []; let line = '';
      // Prefer word boundaries; split oversized words only at grapheme boundaries.
      for (const { segment } of new Intl.Segmenter('ja', { granularity: 'word' }).segment(title)) {
        if (line && ctx.measureText(line + segment).width > width) { lines.push(line.trimEnd()); line = ''; }
        if (ctx.measureText(segment).width > width) {
          for (const { segment: glyph } of new Intl.Segmenter('ja', { granularity: 'grapheme' }).segment(segment)) {
            if (ctx.measureText(line + glyph).width > width) { lines.push(line); line = ''; }
            line += glyph;
          }
        } else line += segment;
      }
      if (line) lines.push(line.trimEnd());
      if (lines.length <= 2) { chosen = { lines, size }; break; }
    }
    if (!chosen) throw new Error('Title needs manual typography review');
    ctx.font = `${square ? 24 : 18}px ${font}`;
    if (ctx.measureText(artist).width > width) throw new Error('Credit needs manual typography review');
    return chosen;
  }, { title, artist, kind, formats, font });
}

export async function study(manifestFile, output, packTitle) {
  if (!packTitle?.trim() || packTitle.length > 300 || /[\u0000-\u001f]/u.test(packTitle)) throw new Error('Provide a plain-text pack title, at most 300 characters');
  const bytes = await readFile(manifestFile), manifest = JSON.parse(bytes);
  if (manifest.schema !== 'ddi-playable-pack/experimental-1' || !manifest.songs?.length) throw new Error('Expected prepared playable-pack manifest');
  const songs = await Promise.all(manifest.songs.map(async song => {
    if (typeof song.title !== 'string' || !song.title.trim()) throw new Error('Missing song title');
    const directory = path.dirname(path.resolve(manifestFile));
    const reference = path.join(directory, `references/${song.id}-0.json`);
    // IDs are filesystem references here; accept only the preparer\'s safe slug.
    if (!/^[A-Za-z0-9_-]+$/.test(song.id)) throw new Error('Unsafe song id');
    const body = await readFile(reference), data = JSON.parse(body), artist = data.sourceMetadata?.artist ?? '';
    if (typeof artist !== 'string' || song.title.length > 300 || artist.length > 300 || /[\u0000-\u001f]/u.test(song.title + artist)) throw new Error('Unsupported title/credit');
    return { id: song.id, title: song.title, artist, seed: hash(Buffer.from(song.sourceAudioSha256 ?? song.title)), referenceSha256: hash(body) };
  }));
  if (new Set(songs.map(s => s.id)).size !== songs.length) throw new Error('Duplicate song id');
  await requireExternalDirectory(output);
  await mkdir(output, { recursive: false });
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const records = [];
  try {
    const context = await browser.newContext({ viewport: { width: 1000, height: 1000 }, deviceScaleFactor: 1, locale: 'ja-JP' });
    await context.route('**/*', route => route.abort());
    const page = await context.newPage();
    const items = [{ id: 'pack', title: packTitle, artist: 'Dancing☆Onigiri', seed: hash(Buffer.from(packTitle + songs.map(s => s.seed).sort().join(''))), kinds: ['pack', 'cover'] }, ...songs.map(s => ({ ...s, kinds: ['banner', 'cover'] }))];
    for (const item of items) {
      const relative = item.referenceSha256 ? `songs/${item.id}` : 'pack';
      const dir = path.join(output, relative); await mkdir(dir, { recursive: true });
      for (const kind of item.kinds) {
        const fitted = await layout(page, item.title, item.artist, kind);
        const body = svg(item.title, item.artist, item.seed, kind, fitted.lines, fitted.size);
        const stem = `${relative}/${kind}`; await writeFile(path.join(output, `${stem}.svg`), body, { flag: 'wx' });
        await page.setContent(`<html lang="ja"><body style="margin:0">${body}</body></html>`);
        await page.evaluate(() => document.fonts.ready);
        const png = await page.locator('svg').screenshot({ path: path.join(output, `${stem}.png`), animations: 'disabled' });
        records.push({ id: item.id, title: item.title, artist: item.artist, role: kind, svg: `${stem}.svg`, png: `${stem}.png`, pngSha256: hash(png), svgSha256: hash(body), dimensions: formats[kind], typography: fitted, sourceReferenceSha256: item.referenceSha256 ?? null });
      }
    }
    const cards = records.map(r => `<figure><img src="${escape(r.png)}" alt="${escape(r.title)}"/><figcaption>${escape(r.title)} · ${escape(r.role)}</figcaption></figure>`).join('');
    await writeFile(path.join(output, 'index.html'), `<!doctype html><html lang="ja"><meta charset="utf-8"><title>Artwork study</title><style>body{background:#11151d;color:#ecebe7;font-family:${font};padding:32px}main{display:grid;grid-template-columns:repeat(auto-fit,minmax(320px,1fr));gap:24px}figure{margin:0}img{display:block;width:100%;height:auto}figcaption{padding:12px 0;color:#acb4c3}</style><h1>Artwork study</h1><p>Generated typography and abstract motifs. Original titles and artist credits. No audio, timing, or simfile changes.</p><main>${cards}</main></html>`, { flag: 'wx' });
    const report = { schema: 'ddi-artwork-study/experimental-1', generatedAt: new Date().toISOString(), sourceManifest: path.resolve(manifestFile), sourceManifestSha256: hash(bytes), generatorSha256: hash(await readFile(fileURLToPath(import.meta.url))), template: 'typographic-lanes/1', browser: browser.version(), platform: process.platform, fontStack: font, fontReproducibility: 'Uses installed fonts; exact pixels may differ across machines. No font files distributed.', packTitle: { value: packTitle, origin: 'caller-provided display label' }, originalAssetsUsed: false, networkRequests: 0, records };
    await writeFile(path.join(output, 'artwork.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
    return report;
  } finally { await browser.close(); }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 5) throw new Error('Usage: node artwork.mjs PREPARED_MANIFEST NEW_EXTERNAL_DIRECTORY PACK_TITLE');
    const report = await study(process.argv[2], path.resolve(process.argv[3]), process.argv[4]);
    console.log(JSON.stringify({ images: report.records.length, output: path.resolve(process.argv[3]) }));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
