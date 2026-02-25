#!/usr/bin/env node
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const https = require('node:https');
const { pipeline } = require('node:stream/promises');

const pkg = require('../package.json');

const REPO = process.env.TRACE_SHARE_NPM_REPO || 'frumu-ai/trace-share';
const VERSION = pkg.version;
const TAG = `v${VERSION}`;

const PLATFORM_MAP = {
  linux: {
    x64: { asset: 'trace-share-linux-x64', bin: 'trace-share' },
    arm64: { asset: 'trace-share-linux-arm64', bin: 'trace-share' },
  },
  darwin: {
    x64: { asset: 'trace-share-darwin-x64', bin: 'trace-share' },
    arm64: { asset: 'trace-share-darwin-arm64', bin: 'trace-share' },
  },
  win32: {
    x64: { asset: 'trace-share-windows-x64.exe', bin: 'trace-share.exe' },
  },
};

function getTarget() {
  const platform = PLATFORM_MAP[process.platform];
  if (!platform) {
    return null;
  }
  return platform[process.arch] || null;
}

function releaseUrl(assetName) {
  return `https://github.com/${REPO}/releases/download/${TAG}/${assetName}`;
}

function checksumFromBody(body, assetName) {
  const lines = body.split(/\r?\n/).map((v) => v.trim()).filter(Boolean);
  for (const line of lines) {
    const match = line.match(/^([a-fA-F0-9]{64})\s+[* ]?(.+)$/);
    if (match && match[2] === assetName) {
      return match[1].toLowerCase();
    }
  }
  return null;
}

function sha256File(filePath) {
  return new Promise((resolve, reject) => {
    const hash = crypto.createHash('sha256');
    const input = fs.createReadStream(filePath);
    input.on('error', reject);
    hash.on('error', reject);
    input.on('data', (chunk) => hash.update(chunk));
    input.on('end', () => resolve(hash.digest('hex').toLowerCase()));
  });
}

function httpGet(url) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, { headers: { 'user-agent': 'trace-share-npm-installer' } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        return resolve(httpGet(res.headers.location));
      }
      if (res.statusCode !== 200) {
        res.resume();
        return reject(new Error(`GET ${url} failed with status ${res.statusCode}`));
      }
      resolve(res);
    });
    req.on('error', reject);
  });
}

async function downloadTo(url, filePath) {
  const res = await httpGet(url);
  await pipeline(res, fs.createWriteStream(filePath));
}

async function readText(url) {
  const res = await httpGet(url);
  const chunks = [];
  for await (const chunk of res) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString('utf8');
}

async function main() {
  const target = getTarget();
  if (!target) {
    console.warn(`trace-share: unsupported platform ${process.platform}-${process.arch}, skipping install`);
    return;
  }

  const nativeDir = path.join(__dirname, '..', 'native', `${process.platform}-${process.arch}`);
  const binPath = path.join(nativeDir, target.bin);
  await fsp.mkdir(nativeDir, { recursive: true });

  const tmpPath = path.join(nativeDir, `${target.bin}.download`);
  const assetUrl = releaseUrl(target.asset);
  const sumUrl = releaseUrl(`${target.asset}.sha256`);

  console.log(`trace-share: downloading ${target.asset} from ${assetUrl}`);
  await downloadTo(assetUrl, tmpPath);

  const checksumBody = await readText(sumUrl);
  const expected = checksumFromBody(checksumBody, target.asset);
  if (!expected) {
    throw new Error(`could not parse checksum for ${target.asset}`);
  }

  const actual = await sha256File(tmpPath);
  if (actual !== expected) {
    throw new Error(`checksum mismatch for ${target.asset}: expected ${expected}, got ${actual}`);
  }

  await fsp.rename(tmpPath, binPath);
  if (process.platform !== 'win32') {
    await fsp.chmod(binPath, 0o755);
  }
  console.log(`trace-share: installed ${binPath}`);
}

main().catch((err) => {
  console.error(`trace-share: install failed: ${err.message}`);
  process.exit(1);
});
