#!/usr/bin/env node
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

const candidate = path.join(__dirname, '..', 'native', process.platform + '-' + process.arch, process.platform === 'win32' ? 'trace-share.exe' : 'trace-share');

if (!fs.existsSync(candidate)) {
  console.error('trace-share binary not found for this platform:', candidate);
  console.error('Try reinstalling package: npm i -g @frumu-ai/trace-share');
  process.exit(1);
}

const result = spawnSync(candidate, process.argv.slice(2), {
  stdio: 'inherit',
});

if (result.error) {
  console.error('trace-share binary not found for this platform:', candidate);
  process.exit(1);
}

process.exit(result.status ?? 0);
