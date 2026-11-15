#!/usr/bin/env node
// Runs a WASI (preview 1) module under Node: wasi-run.mjs MODULE ARGS...
//
// Cargo's runner for `wasm32-wasip1` (.cargo/config.toml), so that tests
// which must give the same answers on every device run built for the
// browser's architecture too, with nothing to install but Node:
//
//   cargo test -p ddi-session --target wasm32-wasip1 --lib cpus::tests
//
// The module gets its arguments and nothing else: no environment, no
// files (what those tests read is compiled in).
import { WASI } from 'node:wasi';
import fs from 'node:fs';

const [, , wasm, ...args] = process.argv;
if (!wasm) {
  console.error('usage: wasi-run.mjs MODULE [ARGS...]');
  process.exit(2);
}
const wasi = new WASI({ version: 'preview1', args: [wasm, ...args], env: {}, preopens: {} });
const mod = await WebAssembly.compile(fs.readFileSync(wasm));
const inst = await WebAssembly.instantiate(mod, wasi.getImportObject());
process.exitCode = wasi.start(inst);
