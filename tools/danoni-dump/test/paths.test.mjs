import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readdir, rm, symlink } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { capture } from '../dump.mjs';
import { startFixture } from './fixture.mjs';

test('capture rejects nested cache symlinks before fetching or writing through them', async () => {
  const scratch = await mkdtemp(path.join(os.tmpdir(), 'ddi-danoni-paths-'));
  const fixture = await startFixture();
  try {
    const cacheDir = path.join(scratch, 'cache');
    const target = path.join(scratch, 'checkout', 'assets');
    await mkdir(path.join(scratch, 'checkout', '.git'), { recursive: true });
    await mkdir(target);
    await mkdir(path.join(cacheDir, 'blobs'), { recursive: true });
    await symlink(target, path.join(cacheDir, 'blobs', 'sha256'));
    await assert.rejects(capture({
      url: fixture.url, outDir: path.join(scratch, 'dump'), cacheDir,
    }), /Symlinks are not allowed/);
    assert.deepEqual(fixture.requests, []);
    assert.deepEqual(await readdir(target), []);
    assert.deepEqual((await readdir(scratch)).sort(), ['cache', 'checkout']);
  } finally {
    await fixture.close();
    await rm(scratch, { recursive: true, force: true });
  }
});

test('capture rejects existing output and cache directories in a Git checkout before fetching', async () => {
  const scratch = await mkdtemp(path.join(os.tmpdir(), 'ddi-danoni-paths-'));
  const fixture = await startFixture();
  try {
    const checkout = path.join(scratch, 'checkout');
    const nested = path.join(checkout, 'existing');
    await mkdir(path.join(checkout, '.git'), { recursive: true });
    await mkdir(nested);
    const alias = path.join(scratch, 'alias');
    await symlink(checkout, alias);
    for (const directory of [nested, path.join(alias, 'existing')]) {
      await assert.rejects(capture({
        url: fixture.url, outDir: directory,
      }), /outside a Git checkout/);
      await assert.rejects(capture({
        url: fixture.url, outDir: path.join(scratch, 'dump'), cacheDir: directory,
      }), /outside a Git checkout/);
    }
    assert.deepEqual(fixture.requests, []);
    assert.deepEqual(await readdir(nested), []);
    assert.deepEqual((await readdir(scratch)).sort(), ['alias', 'checkout']);
  } finally {
    await fixture.close();
    await rm(scratch, { recursive: true, force: true });
  }
});
