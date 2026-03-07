import fs from "node:fs";
import path from "node:path";
import { createState, toSerializable, validateTsumePuzzle } from "../src/shogi-core.js";

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    count: 100,
    seed: Date.now() % 2147483647,
    attempts: 0,
  };
  for (const a of args) {
    if (a.startsWith("--count=")) out.count = Number(a.split("=")[1]);
    if (a.startsWith("--seed=")) out.seed = Number(a.split("=")[1]);
    if (a.startsWith("--attempts=")) out.attempts = Number(a.split("=")[1]);
  }
  return out;
}

function rng(seed) {
  let x = seed || 123456789;
  return () => {
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    return ((x >>> 0) % 1000000) / 1000000;
  };
}

function ri(rand, min, max) {
  return Math.floor(rand() * (max - min + 1)) + min;
}

function loadCurated() {
  const file = "data/curated-puzzles.json";
  if (!fs.existsSync(file)) {
    return { 3: [], 5: [], 7: [], 9: [] };
  }
  return JSON.parse(fs.readFileSync(file, "utf-8"));
}

function randomCandidate(rand, len) {
  const used = new Set();
  const occ = (x, y) => used.has(`${x},${y}`);
  const put = (x, y) => used.add(`${x},${y}`);

  const pieces = [];
  const dk = { x: ri(rand, 3, 7), y: ri(rand, 1, 2), owner: "defender", type: "K" };
  const ak = { x: ri(rand, 7, 9), y: ri(rand, 7, 9), owner: "attacker", type: "K" };
  if (Math.abs(dk.x - ak.x) <= 1 && Math.abs(dk.y - ak.y) <= 1) return null;
  put(dk.x, dk.y);
  put(ak.x, ak.y);
  pieces.push(ak, dk);

  const atkTypes = len >= 9 ? ["R", "B", "G", "S", "P"] : ["R", "G", "S", "P"];
  const defTypes = ["G", "S", "P"];

  for (const t of atkTypes) {
    let x = ri(rand, Math.max(1, dk.x - 3), Math.min(9, dk.x + 3));
    let y = ri(rand, 2, 8);
    let g = 0;
    while (occ(x, y) && g < 30) {
      x = ri(rand, Math.max(1, dk.x - 3), Math.min(9, dk.x + 3));
      y = ri(rand, 2, 8);
      g += 1;
    }
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "attacker", type: t });
  }

  const dn = len >= 7 ? 2 : 1;
  for (let i = 0; i < dn; i += 1) {
    const t = defTypes[ri(rand, 0, defTypes.length - 1)];
    let x = ri(rand, Math.max(1, dk.x - 2), Math.min(9, dk.x + 2));
    let y = ri(rand, 1, 4);
    let g = 0;
    while (occ(x, y) && g < 30) {
      x = ri(rand, Math.max(1, dk.x - 2), Math.min(9, dk.x + 2));
      y = ri(rand, 1, 4);
      g += 1;
    }
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "defender", type: t });
  }

  return {
    pieces,
    hands: {
      attacker: { R: 0, B: 0, G: 0, S: 0, P: 0 },
      defender: { R: 0, B: 0, G: 0, S: 0, P: 0 },
    },
    sideToMove: "attacker",
  };
}

function validateState(initial, mateLength) {
  const st = createState(initial);
  const res = validateTsumePuzzle(st, mateLength);
  if (!res.ok) return null;
  return {
    initial: toSerializable(st),
    solution: res.principalVariation,
  };
}

function searchExtra(rand, mateLength, attempts, existingSignatures) {
  const out = [];
  for (let i = 0; i < attempts; i += 1) {
    const cand = randomCandidate(rand, mateLength);
    if (!cand) continue;
    const sig = JSON.stringify(cand);
    if (existingSignatures.has(sig)) continue;
    const ok = validateState(cand, mateLength);
    if (!ok) continue;
    existingSignatures.add(sig);
    out.push(ok);
  }
  return out;
}

function expand(base, count, mateLength) {
  if (base.length === 0) return [];
  const out = [];
  for (let i = 0; i < count; i += 1) {
    const src = base[i % base.length];
    out.push({
      id: i + 1,
      mateLength,
      initial: src.initial,
      solution: src.solution,
      quality: "validated",
    });
  }
  return out;
}

function writeJson(file, data) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(data, null, 2), "utf-8");
}

function main() {
  const { count, seed, attempts } = parseArgs();
  const rand = rng(seed);
  const curated = loadCurated();

  for (const n of [3, 5, 7, 9]) {
    const curatedValidated = (curated[String(n)] || [])
      .map((x) => validateState(x, n))
      .filter(Boolean);

    const sigs = new Set(curatedValidated.map((p) => JSON.stringify(p.initial)));
    const found = searchExtra(rand, n, attempts, sigs);
    const base = [...curatedValidated, ...found];
    const puzzles = expand(base, count, n);

    writeJson(`puzzles/${n}.json`, puzzles);
    writeJson(`docs/puzzles/${n}.json`, puzzles);

    console.log(`${n}手詰: validated-base=${base.length} expanded=${puzzles.length} attempts=${attempts}`);
  }

  console.log(`seed=${seed}`);
}

main();
