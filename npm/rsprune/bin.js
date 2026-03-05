#!/usr/bin/env node

const { spawnSync } = require('child_process');
const path = require('path');

const PLATFORM_MAP = {
  'darwin-arm64': 'rsprune-darwin-arm64',
  'darwin-x64':   'rsprune-darwin-x64',
  'linux-x64':    'rsprune-linux-x64',
  'linux-arm64':  'rsprune-linux-arm64',
  'win32-x64':    'rsprune-win32-x64',
};

const key = `${process.platform}-${process.arch}`;
const pkg = PLATFORM_MAP[key];

if (!pkg) {
  console.error(`rsprune: unsupported platform: ${key}`);
  process.exit(1);
}

const binName = process.platform === 'win32' ? 'rsprune.exe' : 'rsprune';

let binPath;
try {
  binPath = require.resolve(`${pkg}/bin/${binName}`);
} catch {
  console.error(`rsprune: could not find platform package "${pkg}". Try reinstalling rsprune.`);
  process.exit(1);
}

const result = spawnSync(binPath, process.argv.slice(2), { stdio: 'inherit' });
process.exit(result.status ?? 1);
