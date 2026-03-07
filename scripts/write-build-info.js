import fs from 'node:fs';
import { execSync } from 'node:child_process';

function safe(cmd, fallback = 'unknown') {
  try {
    return execSync(cmd, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
  } catch {
    return fallback;
  }
}

const branch = safe('git branch --show-current');
const commit = safe('git rev-parse --short HEAD');
const builtAt = new Date().toISOString();

const info = { branch, commit, builtAt };
fs.writeFileSync('docs/build-info.json', JSON.stringify(info, null, 2) + '\n', 'utf-8');

// index.html のキャッシュバスト値を commit hash で更新する
const indexPath = 'docs/index.html';
let html = fs.readFileSync(indexPath, 'utf-8');
html = html.replace(/\?v=[a-z0-9]+/g, `?v=${commit}`);
fs.writeFileSync(indexPath, html, 'utf-8');

console.log('wrote docs/build-info.json', info);
