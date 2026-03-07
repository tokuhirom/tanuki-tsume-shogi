import fs from "node:fs";
import path from "node:path";
import { createState, toSerializable, validateTsumePuzzle } from "../src/shogi-core.js";

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    count: 100,
    seed: Date.now() % 2147483647,
    attempts3: 0,
    attempts5: 0,
    attempts79: 0,
  };
  for (const a of args) {
    if (a.startsWith("--count=")) out.count = Number(a.split("=")[1]);
    if (a.startsWith("--seed=")) out.seed = Number(a.split("=")[1]);
    if (a.startsWith("--attempts=")) {
      const v = Number(a.split("=")[1]);
      out.attempts3 = v;
      out.attempts5 = Math.floor(v / 3);
    }
    if (a.startsWith("--attempts3=")) out.attempts3 = Number(a.split("=")[1]);
    if (a.startsWith("--attempts5=")) out.attempts5 = Number(a.split("=")[1]);
    if (a.startsWith("--attempts79=")) out.attempts79 = Number(a.split("=")[1]);
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

function pick(rand, arr) {
  return arr[ri(rand, 0, arr.length - 1)];
}

function loadCurated() {
  const file = "data/curated-puzzles.json";
  if (!fs.existsSync(file)) {
    return { 3: [], 5: [], 7: [], 9: [] };
  }
  return JSON.parse(fs.readFileSync(file, "utf-8"));
}

function emptyHands() {
  return {
    attacker: { R: 0, B: 0, G: 0, S: 0, P: 0 },
    defender: { R: 0, B: 0, G: 0, S: 0, P: 0 },
  };
}

function cloneInitial(initial) {
  return {
    pieces: initial.pieces.map((p) => ({ ...p })),
    hands: {
      attacker: { ...initial.hands.attacker },
      defender: { ...initial.hands.defender },
    },
    sideToMove: initial.sideToMove,
  };
}

function uniquePieces(pieces) {
  const seen = new Set();
  for (const p of pieces) {
    const k = `${p.x},${p.y}`;
    if (seen.has(k)) return false;
    seen.add(k);
  }
  return true;
}

function basicValidity(initial) {
  if (!initial || !Array.isArray(initial.pieces)) return false;
  if (!initial.pieces.every((p) => p.x >= 1 && p.x <= 9 && p.y >= 1 && p.y <= 9)) return false;
  if (!uniquePieces(initial.pieces)) return false;

  const ak = initial.pieces.find((p) => p.owner === "attacker" && p.type === "K");
  const dk = initial.pieces.find((p) => p.owner === "defender" && p.type === "K");
  if (!ak || !dk) return false;
  if (Math.abs(ak.x - dk.x) <= 1 && Math.abs(ak.y - dk.y) <= 1) return false;
  return true;
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
    hands: emptyHands(),
    sideToMove: "attacker",
  };
}

function mutateInitial(rand, seed, len) {
  const cand = cloneInitial(seed);
  const ops = ["move-piece", "move-piece", "move-piece", "add-piece", "tweak-hand"];
  const op = pick(rand, ops);

  if (op === "move-piece") {
    const movable = cand.pieces.filter((p) => p.type !== "K");
    if (movable.length === 0) return null;
    const target = pick(rand, movable);
    const dx = pick(rand, [-2, -1, 1, 2]);
    const dy = pick(rand, [-2, -1, 1, 2]);
    target.x += dx;
    target.y += dy;
  }

  if (op === "add-piece") {
    const owner = pick(rand, ["attacker", "defender"]);
    const type = owner === "attacker"
      ? pick(rand, len >= 9 ? ["R", "B", "G", "S", "P"] : ["R", "G", "S", "P"])
      : pick(rand, ["G", "S", "P"]);
    cand.pieces.push({ x: ri(rand, 1, 9), y: ri(rand, 1, 9), owner, type });

    if (cand.pieces.length > 12) {
      const removable = cand.pieces.filter((p) => p.type !== "K");
      if (removable.length > 0) {
        const rm = pick(rand, removable);
        cand.pieces = cand.pieces.filter((p) => !(p.x === rm.x && p.y === rm.y && p.owner === rm.owner && p.type === rm.type));
      }
    }
  }

  if (op === "tweak-hand") {
    const owner = "attacker";
    const t = pick(rand, ["P", "S", "G"]);
    cand.hands[owner][t] = ri(rand, 0, 1);
  }

  return basicValidity(cand) ? cand : null;
}

function scorePuzzle(initial, solution) {
  const atkTypes = new Set(initial.pieces.filter((p) => p.owner === "attacker" && p.type !== "K").map((p) => p.type)).size;
  const pieceCount = initial.pieces.length;
  const attackerMoves = solution.filter((_, i) => i % 2 === 0);
  const dropCount = attackerMoves.filter((m) => m.drop).length;
  const promoteCount = attackerMoves.filter((m) => m.promote).length;
  const uniqueTargets = new Set(attackerMoves.map((m) => `${m.to[0]},${m.to[1]}`)).size;

  return (
    atkTypes * 2 +
    dropCount * 5 +
    promoteCount * 3 +
    uniqueTargets * 2 -
    Math.max(0, pieceCount - 9)
  );
}

function validateState(initial, mateLength) {
  const st = createState(initial);
  const res = validateTsumePuzzle(st, mateLength);
  if (!res.ok) return null;
  const serial = toSerializable(st);
  return {
    initial: serial,
    solution: res.principalVariation,
    score: scorePuzzle(serial, res.principalVariation),
  };
}

function collectValidated(rand, mateLength, attempts, seedBases, existingSignatures) {
  const out = [];
  if (attempts <= 0) return out;

  const fromSeed = Math.floor(attempts * 0.75);
  for (let i = 0; i < fromSeed; i += 1) {
    if (seedBases.length === 0) break;
    const seed = pick(rand, seedBases).initial;
    const cand = mutateInitial(rand, seed, mateLength);
    if (!cand) continue;
    const ok = validateState(cand, mateLength);
    if (!ok) continue;
    const sig = JSON.stringify(ok.initial);
    if (existingSignatures.has(sig)) continue;
    existingSignatures.add(sig);
    out.push(ok);
  }

  for (let i = fromSeed; i < attempts; i += 1) {
    const cand = randomCandidate(rand, mateLength);
    if (!cand) continue;
    const ok = validateState(cand, mateLength);
    if (!ok) continue;
    const sig = JSON.stringify(ok.initial);
    if (existingSignatures.has(sig)) continue;
    existingSignatures.add(sig);
    out.push(ok);
  }

  out.sort((a, b) => b.score - a.score);
  return out;
}

function transformInitial(initial, transformFn) {
  const pieces = initial.pieces.map((p) => {
    const [x, y] = transformFn(p.x, p.y);
    return { ...p, x, y };
  });
  const candidate = { pieces, hands: initial.hands, sideToMove: initial.sideToMove };
  return basicValidity(candidate) ? candidate : null;
}

function makeVariants(baseValidated, mateLength) {
  const variants = [];
  const seen = new Set();

  const add = (initialLike) => {
    if (!initialLike) return;
    const ok = validateState(initialLike, mateLength);
    if (!ok) return;
    const sig = JSON.stringify(ok.initial);
    if (seen.has(sig)) return;
    seen.add(sig);
    variants.push(ok);
  };

  for (const b of baseValidated) {
    add(b.initial);
    add(transformInitial(b.initial, (x, y) => [10 - x, y]));
    for (const dx of [-2, -1, 1, 2]) {
      add(transformInitial(b.initial, (x, y) => [x + dx, y]));
    }
    for (const dy of [1, 2]) {
      add(transformInitial(b.initial, (x, y) => [x, y + dy]));
    }
  }

  variants.sort((a, b) => b.score - a.score);
  return variants;
}

function weightedPick(rand, items) {
  if (items.length === 0) return null;
  const weights = items.map((i) => Math.max(1, i.score + 10));
  const sum = weights.reduce((a, b) => a + b, 0);
  let r = rand() * sum;
  for (let i = 0; i < items.length; i += 1) {
    r -= weights[i];
    if (r <= 0) return items[i];
  }
  return items[items.length - 1];
}

function expand(base, count, mateLength, rand) {
  if (base.length === 0) return [];
  const pool = makeVariants(base, mateLength);
  const srcPool = pool.length > 0 ? pool : base;
  const out = [];
  let prevSig = "";

  for (let i = 0; i < count; i += 1) {
    let src = weightedPick(rand, srcPool);
    if (!src) break;

    const sig = JSON.stringify(src.initial);
    if (sig === prevSig && srcPool.length > 1) {
      const alt = srcPool.find((p) => JSON.stringify(p.initial) !== prevSig);
      if (alt) src = alt;
    }
    prevSig = JSON.stringify(src.initial);

    out.push({
      id: i + 1,
      mateLength,
      initial: src.initial,
      solution: src.solution,
      quality: "validated",
      score: src.score,
    });
  }

  return out;
}

function writeJson(file, data) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(data, null, 2), "utf-8");
}

function main() {
  const { count, seed, attempts3, attempts5, attempts79 } = parseArgs();
  const rand = rng(seed);
  const curated = loadCurated();

  for (const n of [3, 5]) {
    const curatedValidated = (curated[String(n)] || [])
      .map((x) => validateState(x, n))
      .filter(Boolean);

    const sigs = new Set(curatedValidated.map((p) => JSON.stringify(p.initial)));
    const attempts = n === 3 ? attempts3 : n === 5 ? attempts5 : attempts79;
    const found = collectValidated(rand, n, attempts, curatedValidated, sigs);
    const base = [...curatedValidated, ...found];
    const puzzles = expand(base, count, n, rand);

    writeJson(`puzzles/${n}.json`, puzzles);
    writeJson(`docs/puzzles/${n}.json`, puzzles);

    console.log(`${n}手詰: validated-base=${base.length} expanded=${puzzles.length} attempts=${attempts}`);
  }

  console.log(`seed=${seed}`);
}

main();
