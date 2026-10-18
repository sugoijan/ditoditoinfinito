import { chromium } from 'playwright-core';
import { EngineObserver } from './adapter.mjs';
import { decodeSnapshot } from './serialization.mjs';

const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

async function waitFor(predicate, timeoutMs, description) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await delay(50);
  }
  throw new Error(`Timed out waiting for ${description}`);
}

export async function captureBrowser({ url, network, chartIds, trustedHashes = [], timeoutMs = 120000, onProgress = () => {} }) {
  const browser = await chromium.launch({ channel: 'chrome', headless: true, chromiumSandbox: true,
    args: ['--autoplay-policy=no-user-gesture-required'] });
  const result = { browserVersion: browser.version(), charts: [], diagnostics: [], catalog: null };
  const queue = chartIds ? [...chartIds] : [0];
  try {
    for (let queueIndex = 0; queueIndex < queue.length; queueIndex++) {
      const chartId = queue[queueIndex];
      onProgress(`Opening chart ${chartId}`);
      const context = await browser.newContext({ serviceWorkers: 'block', acceptDownloads: false,
        locale: 'en-US', viewport: { width: 1280, height: 960 } });
      const observer = new EngineObserver({ trustedHashes });
      const entry = { chartId, status: 'partial', diagnostics: [], inputs: [], snapshots: [], scripts: [] };
      result.charts.push(entry);
      try {
        await network.attach(context);
        // This alias is specific to reviewed CW engine cache-busters. Full
        // semantic queries (dos, scoreId, stageNum, key, version) remain intact.
        await context.route('**/*', async route => {
          const request = route.request();
          if (request.frame().parentFrame()) {
            if (request.resourceType() === 'document') entry.diagnostics.push({ code: 'excluded-subframe', message: 'Embedded document is outside the top-level CW capture scope', url: request.url() });
            await route.abort('blockedbyclient');
            return;
          }
          const requested = new URL(request.url());
          const canonical = new URL(requested); canonical.search = '';
          if (/^\?\d{13}$/.test(requested.search) &&
              ((observer.scripts.length && ['script', 'stylesheet', 'image', 'media'].includes(request.resourceType())) ||
              observer.cacheBusterUrls?.has(canonical.href))) {
            try { network.addAlias(requested.href, canonical.href); }
            catch (error) {
              entry.diagnostics.push({ code: 'alias-rejected', message: error.message, url: requested.href });
              await route.abort('blockedbyclient');
              return;
            }
          }
          await route.fallback();
        });
        const page = await context.newPage();
        page.on('pageerror', error => entry.diagnostics.push({ code: 'page-error', message: error.message, stack: error.stack }));
        page.on('console', message => {
          if (['error', 'warning'].includes(message.type())) entry.diagnostics.push({ code: 'console-' + message.type(), message: message.text().slice(0, 2000) });
        });
        page.setDefaultTimeout(timeoutMs);
        await observer.attach(page);
        await page.goto(url, { waitUntil: 'domcontentloaded', timeout: timeoutMs });
        await page.waitForFunction(() => typeof g_headerObj !== 'undefined' && g_headerObj.keyLabels?.length &&
          typeof setDifficulty === 'function');
        if (observer.diagnostics.some(d => d.code === 'unsupported-engine')) throw new Error('Engine source is not registered; inspect the captured source hash before adding support');
        entry.pageUrl = page.url();
        const catalog = await page.evaluate(() => ({
          count: g_headerObj.keyLabels.length,
          keyModes: g_headerObj.keyLabels,
          version: g_version,
          creditText: document.body.innerText,
          termsLinks: [...document.querySelectorAll('a[href]')].filter(a =>
            /terms|利用規約|利用条件|免責|著作権|copyright/i.test(a.textContent)).map(a => ({ text: a.textContent, url: a.href })),
        }));
        result.catalog ??= catalog;
        if (!chartIds && queueIndex === 0) queue.push(...Array.from({ length: catalog.count - 1 }, (_, i) => i + 1));
        if (chartId >= catalog.count) throw new Error(`Chart ${chartId} is out of range (count ${catalog.count})`);
        if (await page.locator('#btnStart').count()) await page.locator('#btnStart').click();
        await page.waitForFunction(() => document.querySelector('#btnPlay, #lblMusicNumBarSlider'));
        if (await page.locator('#lblMusicNumBarSlider').count()) {
          // The public music-select extension inserts a song menu before options.
          // Use its DOM input and Enter handler so its own selection hooks run.
          await page.evaluate(id => {
            const slider = document.querySelector('#lblMusicNumBarSlider');
            slider.value = String(g_headerObj.musicNos[id]);
            slider.dispatchEvent(new Event('input', { bubbles: true }));
          }, chartId);
          await page.keyboard.press('Enter');
        }
        await page.locator('#btnPlay').waitFor({ state: 'visible' });
        await page.evaluate(id => {
          g_stateObj.scoreId = id;
          setDifficulty(true);
          Object.assign(g_stateObj, { adjustment: 0, fadein: 0, shuffle: 'OFF', motion: 'OFF',
            reverse: 'OFF', scroll: '---', autoPlay: 'OFF',
            d_velocity: 'ON', d_color: 'ON', d_background: 'ON', d_arroweffect: 'ON' });
          if (typeof g_colorType !== 'undefined') g_colorType = 'Type0';
        }, chartId);
        await page.locator('#btnPlay').click();
        await waitFor(() => entry.diagnostics.some(d => /Loading error:/.test(d.message)) || observer.snapshots.some(encoded => {
          const s = decodeSnapshot(encoded);
          return s.chartId === chartId && s.stage === 'prepared';
        }), timeoutMs, 'full parse and prepared state');
        await waitFor(() => observer.snapshots.some(s => decodeSnapshot(s).stage === 'runtime') ||
          entry.diagnostics.some(d => /Loading error:/.test(d.message)), timeoutMs, 'audio loading and main entry');
        const loadError = entry.diagnostics.find(d => /Loading error:/.test(d.message));
        if (loadError) throw new Error(loadError.message);
        await page.waitForFunction(() => typeof g_currentPage !== 'undefined' && g_currentPage === 'main', { }, { timeout: timeoutMs });
        await observer.pending;
        entry.media = await page.evaluate(() => {
          let data = typeof g_musicdata === 'undefined' ? '' : g_musicdata;
          const fullMusicUrl = typeof getFullMusicUrl === 'function' && typeof getMusicUrl === 'function'
            ? new URL(getFullMusicUrl(getMusicUrl(g_stateObj.scoreId)), location.href).href : null;
          // The site already executed musicInit in Chrome. We only decode its
          // resulting data URI here; no downloaded script is executed in Node.
          let decoded = null;
          if (typeof data === 'string' && data.startsWith('data:')) {
            const match = data.match(/^data:([^;,]*)(;base64)?,([\s\S]*)$/);
            if (match) {
              const raw = match[2] ? atob(match[3]) : decodeURIComponent(match[3]);
              decoded = { mimeType: match[1] || 'application/octet-stream', base64: btoa(raw) };
            }
          } else if (typeof data === 'string' && data.trim()) {
            const raw = atob(data);
            const mimeType = raw.startsWith('RIFF') ? 'audio/wav' : raw.startsWith('OggS') ? 'audio/ogg'
              : raw.startsWith('fLaC') ? 'audio/flac' : raw.startsWith('ID3') || (raw.charCodeAt(0) === 255 && (raw.charCodeAt(1) & 224) === 224)
                ? 'audio/mpeg' : 'application/octet-stream';
            decoded = { mimeType, base64: btoa(raw) };
          }
          const assets = new Set([...document.images].map(img => img.currentSrc || img.src));
          for (const link of document.querySelectorAll('link[rel="preload"]')) assets.add(link.href);
          const addCss = (css, base = document.baseURI) => {
            for (const match of css.matchAll(/url\(\s*["']?([^"')]+)["']?\s*\)/g)) {
              try { assets.add(new URL(match[1].trim(), base).href); } catch { /* retained native CSS */ }
            }
          };
          const visited = new Set();
          const scan = value => {
            if (!value || typeof value !== 'object' || visited.has(value)) return;
            visited.add(value);
            for (const [key, descriptor] of Object.entries(Object.getOwnPropertyDescriptors(value))) {
              if (!('value' in descriptor)) continue;
              const child = descriptor.value;
              if (key === 'htmlText' && typeof child === 'string') {
                const template = document.createElement('template'); template.innerHTML = child;
                for (const el of template.content.querySelectorAll('[src]')) {
                  const src = el.getAttribute('src');
                  if (src) assets.add(new URL(src, document.baseURI).href);
                }
                addCss(child);
              } else if (child && typeof child === 'object') scan(child);
            }
          };
          scan(g_scoreObj); scan(g_headerObj);
          const rules = (list, base) => {
            for (const rule of list) {
              addCss(rule.cssText, base);
              if (rule.cssRules) rules(rule.cssRules, base);
            }
          };
          for (const sheet of document.styleSheets) {
            try { rules(sheet.cssRules, sheet.href || document.baseURI); } catch { /* cross-origin CSS captured as resource */ }
          }
          return { fullMusicUrl, decoded,
            images: [...assets].filter(src => /^https?:/.test(src)),
            effectiveOptions: { ...g_stateObj }, locale: navigator.language,
            viewport: { width: innerWidth, height: innerHeight },
            customHooks: typeof g_customJsObj === 'undefined' ? [] : Object.keys(g_customJsObj)
              .filter(key => Array.isArray(g_customJsObj[key]) && g_customJsObj[key].length),
          };
        });
        // Every image currently referenced by the DOM must be in the cache,
        // including those still queued by the page when the parser completed.
        for (const image of [...new Set(entry.media.images)]) {
          const { record: resource } = await network.fetch(image, { role: 'image' });
          if (resource.status < 200 || resource.status >= 300 || !resource.completeBody) {
            throw new Error(`Required visual resource is unavailable: HTTP ${resource.status} ${image}`);
          }
        }
        await waitFor(() => network.inflight.size === 0, timeoutMs, 'captured resource requests to settle');
        entry.status = entry.diagnostics.some(d => ['page-error', 'alias-rejected'].includes(d.code)) || observer.diagnostics.length ? 'partial' : 'complete';
        onProgress(`Captured chart ${chartId}`);
      } catch (error) {
        entry.diagnostics.push({ code: 'chart-capture-failed', message: error.message });
        onProgress(`Chart ${chartId}: ${error.message}`);
      } finally {
        entry.inputs = observer.inputs;
        entry.snapshots = observer.snapshots;
        entry.scripts = observer.scripts;
        entry.diagnostics.push(...observer.diagnostics);
        await observer.close().catch(() => {});
        await context.close();
      }
      if (!result.catalog && !chartIds) break;
    }
  } finally {
    await browser.close();
  }
  return result;
}
