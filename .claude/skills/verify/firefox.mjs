// Headless Firefox for the verify recipes, with everything Firefox writes kept
// in a temporary folder. Pass Playwright's `firefox` so this file needs no
// node_modules of its own:
//
//   import { firefox } from 'playwright-core';
//   import { launchFirefox } from '<repo>/.claude/skills/verify/firefox.mjs';
//   const browser = await launchFirefox(firefox);                   // Playwright's build
//   const browser = await launchFirefox(firefox, { stock: true });  // /Applications/Firefox.app
//
// On macOS, ~/Library/Application Support/Firefox only opens to Firefox
// itself (a com.apple.macl list); started from a terminal, Firefox runs on
// the terminal's behalf, cannot read its profiles.ini and quits with "Could
// not find profile folder.", even with --profile. MOZ_APP_DATA and
// MOZ_LOCAL_APP_DATA move that root (profiles.ini, crash reports, pending
// pings) to a scratch folder, which also keeps the tests away from the
// maintainer's own profiles.
//
// Options: `stock` drives the installed Firefox over WebDriver BiDi instead of
// Playwright's patched build (`npx playwright-core install firefox`);
// `sound` lets the audio reach the speakers (muted by default; the game's
// output peak in __DDI_DEBUG.audio still reads the real level); `prefs` adds
// Firefox preferences; anything else goes to `firefox.launch`.

import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export async function launchFirefox(firefox, { stock = false, sound = false, prefs = {}, env, ...options } = {}) {
  const data = mkdtempSync(join(tmpdir(), 'ddi-firefox-'));
  const roaming = join(data, 'roaming');
  const local = join(data, 'local');
  mkdirSync(roaming);
  mkdirSync(local);
  const cleanUp = () => rmSync(data, { recursive: true, force: true });
  try {
    const browser = await firefox.launch({
      ...(stock ? { channel: 'moz-firefox' } : {}),
      headless: true,
      ...options,
      env: { ...process.env, ...env, MOZ_APP_DATA: roaming, MOZ_LOCAL_APP_DATA: local },
      firefoxUserPrefs: {
        // The equivalent of Chrome's --autoplay-policy=no-user-gesture-required.
        'media.autoplay.default': 0,
        'media.autoplay.blocking_policy': 0,
        ...(sound ? {} : { 'media.volume_scale': '0.0' }),
        ...prefs,
      },
    });
    browser.on('disconnected', cleanUp);
    return browser;
  } catch (e) {
    cleanUp();
    throw e;
  }
}
