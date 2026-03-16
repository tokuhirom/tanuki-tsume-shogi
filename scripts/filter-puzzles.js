#!/usr/bin/env node
/**
 * パズルフィルタスクリプト
 *
 * puzzles/*.json から詰み筋（解の後半）が類似するパズルを除外し、
 * 多様な問題セットを public/puzzles/*.json に出力する。
 *
 * 生成済みの生データは puzzles/ にそのまま保持し、
 * 実際にユーザーに表出するパズルだけをフィルタリングする。
 */

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

// 詰み筋シグネチャ: 解の後半 tailLen 手を文字列化
function solutionTailSignature(solution, tailLen) {
  const start = Math.max(0, solution.length - tailLen);
  return solution.slice(start).map(m => {
    if (m.drop) {
      return `${m.drop}*${m.to[0]}${m.to[1]}`;
    }
    const from = m.from ? `${m.from[0]}${m.from[1]}` : '??';
    return `${from}-${m.to[0]}${m.to[1]}${m.promote ? '+' : ''}`;
  }).join(',');
}

// 駒構成キー
function compositionKey(initial) {
  const atk = initial.pieces
    .filter(p => p.owner === 'attacker' && p.type !== 'K')
    .map(p => p.type)
    .sort()
    .join('');
  const def = initial.pieces
    .filter(p => p.owner === 'defender' && p.type !== 'K')
    .map(p => p.type)
    .sort()
    .join('');
  const h = initial.hands?.attacker || {};
  return `a:${atk} d:${def} h:${JSON.stringify(h)}`;
}

const MAX_PER_TAIL = 5;       // 同一詰み筋は最大5問
const MAX_PER_COMPOSITION = 5; // 同一駒構成は最大5問（generator側で3に制限済み、ここでは緩めに）
const MATE_LENGTHS = [1, 3, 5, 7, 9, 11];

mkdirSync('public/puzzles', { recursive: true });

let totalInput = 0;
let totalOutput = 0;

for (const ml of MATE_LENGTHS) {
  const inputFile = join('puzzles', `${ml}.json`);
  const outputFile = join('public', 'puzzles', `${ml}.json`);

  let puzzles;
  try {
    puzzles = JSON.parse(readFileSync(inputFile, 'utf-8'));
  } catch {
    console.error(`[skip] ${inputFile} が読めません`);
    continue;
  }

  // 7手詰以上で詰み筋フィルタを適用
  const tailLen = ml >= 7 ? Math.min(ml, 7) : 0;
  const tailCount = new Map();
  const compCount = new Map();
  const filtered = [];

  for (const p of puzzles) {
    // 駒構成チェック
    const ckey = compositionKey(p.initial);
    const cc = compCount.get(ckey) || 0;
    if (cc >= MAX_PER_COMPOSITION) continue;

    // 詰み筋チェック
    if (tailLen > 0 && p.solution) {
      const tkey = solutionTailSignature(p.solution, tailLen);
      const tc = tailCount.get(tkey) || 0;
      if (tc >= MAX_PER_TAIL) continue;
      tailCount.set(tkey, tc + 1);
    }

    compCount.set(ckey, cc + 1);
    filtered.push(p);
  }

  // IDを振り直す
  filtered.forEach((p, i) => { p.id = i + 1; });

  writeFileSync(outputFile, JSON.stringify(filtered, null, 2));
  const removed = puzzles.length - filtered.length;
  console.log(`${ml}手詰: ${puzzles.length}問 → ${filtered.length}問 (${removed}問除外)`);
  totalInput += puzzles.length;
  totalOutput += filtered.length;
}

console.log(`合計: ${totalInput}問 → ${totalOutput}問`);
