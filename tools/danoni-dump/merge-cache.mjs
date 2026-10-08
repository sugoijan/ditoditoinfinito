// Consolidate existing acquisition caches without making any network request.
import { createHash, randomUUID } from 'node:crypto';
import { readFile, writeFile, rename, realpath, lstat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { requireExternalDirectory } from './dump.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const signature = r => JSON.stringify([r.sha256, r.byteLength, r.status,
  r.headers?.['content-type'], r.headers?.vary, r.headers?.location]);
const url = value => {
  const parsed = new URL(value);
  if (!['http:', 'https:'].includes(parsed.protocol) || parsed.username || parsed.password) throw new Error('Expected anonymous cache URL');
};

async function blob(root, record) {
  if (!/^[a-f0-9]{64}$/.test(record.sha256) || record.bodyPath !== `blobs/sha256/${record.sha256}`) throw new Error('Invalid cache blob path');
  const file = path.join(root, record.bodyPath);
  if ((await realpath(file)) !== file || !(await lstat(file)).isFile()) throw new Error('Cache blob must be a regular file without symlinks');
  const bytes = await readFile(file);
  if (hash(bytes) !== record.sha256 || bytes.length !== record.byteLength) throw new Error('Corrupt cache body');
  return bytes;
}

export async function mergeCaches(sourceDir, targetDir) {
  const source = await realpath(sourceDir), target = await realpath(targetDir);
  await requireExternalDirectory(target);
  if (source === target) throw new Error('Use two different cache directories');
  const sourceBytes = await readFile(path.join(source, 'index.json'));
  const targetBytes = await readFile(path.join(target, 'index.json'));
  const a = JSON.parse(sourceBytes), b = JSON.parse(targetBytes);
  if (a.schema !== 'ddi-danoni-cache/1' || b.schema !== a.schema) throw new Error('Unsupported cache schema');
  const entries = new Map(Object.entries(b.entries)), failures = new Map(Object.entries(b.failures ?? {}));
  const aliases = new Map(b.aliases ?? []), bodies = [], duplicates = [];
  // Validate every imported binding before writing. Existing target records win
  // only when the source has the same response identity; retain both histories.
  for (const [key, record] of Object.entries(a.entries)) {
    url(key);
    const bytes = await blob(source, record);
    if (entries.has(key)) {
      const prior = entries.get(key);
      if (signature(prior) !== signature(record)) throw new Error(`Conflicting cached response: ${key}`);
      await blob(target, prior);
      duplicates.push({ url: key, sha256: record.sha256, sourceRetrievedAt: record.retrievedAt, targetRetrievedAt: prior.retrievedAt });
    } else {
      if (failures.has(key)) throw new Error(`Response conflicts with a recorded refusal: ${key}`);
      entries.set(key, record); bodies.push({ record, bytes });
    }
  }
  for (const [key, record] of Object.entries(a.failures ?? {})) {
    url(key);
    if (entries.has(key)) throw new Error(`Refusal conflicts with cached response: ${key}`);
    if (!failures.has(key)) failures.set(key, record);
  }
  for (const [alias, canonical] of a.aliases ?? []) {
    url(alias); url(canonical);
    if (new URL(alias).origin !== new URL(canonical).origin || (aliases.has(alias) && aliases.get(alias) !== canonical)) throw new Error('Conflicting cache alias');
    aliases.set(alias, canonical);
  }
  for (const alias of aliases.keys()) {
    const seen = new Set(); let cursor = alias;
    while (aliases.has(cursor)) {
      if (seen.has(cursor)) throw new Error('Cache alias cycle');
      seen.add(cursor); cursor = aliases.get(cursor);
    }
  }
  for (const { record, bytes } of bodies) {
    const destination = path.join(target, record.bodyPath);
    if (await realpath(path.dirname(destination)) !== path.dirname(destination)) throw new Error('Target blob directory has symlinks');
    try { await writeFile(destination, bytes, { flag: 'wx' }); }
    catch (error) {
      if (error.code !== 'EEXIST') throw error;
      await blob(target, record);
    }
  }
  if (hash(await readFile(path.join(target, 'index.json'))) !== hash(targetBytes)) throw new Error('Cache changed during merge; stop concurrent writers');
  const merged = Buffer.from(JSON.stringify({ schema: a.schema, entries: Object.fromEntries(entries), aliases: [...aliases], failures: Object.fromEntries(failures) }, null, 2) + '\n');
  const report = { schema: 'ddi-danoni-cache-merge/1', mergedAt: new Date().toISOString(), source, target,
    sourceIndexSha256: hash(sourceBytes), targetBeforeSha256: hash(targetBytes), targetAfterSha256: hash(merged),
    imported: bodies.map(({ record }) => record.requestedUrl), duplicates, networkRequests: 0 };
  const id = randomUUID(), temporary = path.join(target, `index-${id}.tmp`);
  await writeFile(temporary, merged, { flag: 'wx' });
  await rename(temporary, path.join(target, 'index.json'));
  await writeFile(path.join(target, `merge-${id}.json`), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  return report;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 4) throw new Error('Usage: node merge-cache.mjs SOURCE_CACHE TARGET_CACHE (no concurrent writers)');
    const report = await mergeCaches(process.argv[2], process.argv[3]);
    console.log(JSON.stringify({ imported: report.imported.length, duplicates: report.duplicates.length, networkRequests: 0 }, null, 2));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
