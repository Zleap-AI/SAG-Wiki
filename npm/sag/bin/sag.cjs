#!/usr/bin/env node
'use strict';
const {spawn} = require('node:child_process');
const {existsSync} = require('node:fs');
const {join} = require('node:path');
const {constants} = require('node:os');
const binary = join(__dirname, '..', 'native', `${process.platform}-${process.arch}`, process.platform === 'win32' ? 'sag.exe' : 'sag');
if (!existsSync(binary)) {
  console.error(`This sag release does not include ${process.platform}-${process.arch}. Check the package's supported platforms.`);
  process.exit(1);
}
const child = spawn(binary, process.argv.slice(2), {stdio: 'inherit'});
const signals = {
  SIGINT: () => child.kill('SIGINT'),
  SIGTERM: () => child.kill('SIGTERM'),
};
for (const [signal, forward] of Object.entries(signals)) process.on(signal, forward);
function stopForwarding() {
  for (const [signal, forward] of Object.entries(signals)) process.off(signal, forward);
}
child.once('error', error => {
  stopForwarding();
  console.error(`Cannot start sag: ${error.message}`);
  process.exitCode = 1;
});
child.once('exit', (code, signal) => {
  stopForwarding();
  process.exitCode = code ?? (128 + constants.signals[signal]);
});
