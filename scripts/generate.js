import fs from "node:fs";
import path from "node:path";
import { createState, toSerializable, validateTsumePuzzle } from "../src/shogi-core.js";

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    maxPerCategory: 100,
    seed: Date.now() % 2147483647,
    attempts3: 8000,
    attempts5: 3000,
  };
  for (const a of args) {
    if (a.startsWith("--max=")) out.maxPerCategory = Number(a.split("=")[1]);
    if (a.startsWith("--seed=")) out.seed = Number(a.split("=")[1]);
    if (a.startsWith("--attempts3=")) out.attempts3 = Number(a.split("=")[1]);
    if (a.startsWith("--attempts5=")) out.attempts5 = Number(a.split("=")[1]);
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
  if (!fs.existsSync(file)) return { 3: [], 5: [] };
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

function stripAttackerKing(initial) {
  return {
    pieces: initial.pieces.filter((p) => !(p.owner === "attacker" && p.type === "K")).map((p) => ({ ...p })),
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

  const dk = initial.pieces.find((p) => p.owner === "defender" && p.type === "K");
  if (!dk) return false;
  return true;
}

function structuralSignature(initial) {
  const dk = initial.pieces.find((p) => p.owner === "defender" && p.type === "K");
  if (!dk) return "";

  const relBase = initial.pieces
    .map((p) => ({ owner: p.owner, type: p.type, dx: p.x - dk.x, dy: p.y - dk.y }))
    .sort((a, b) =>
      a.owner.localeCompare(b.owner) ||
      a.type.localeCompare(b.type) ||
      a.dy - b.dy ||
      a.dx - b.dx
    );

  const relMirror = initial.pieces
    .map((p) => ({ owner: p.owner, type: p.type, dx: -(p.x - dk.x), dy: p.y - dk.y }))
    .sort((a, b) =>
      a.owner.localeCompare(b.owner) ||
      a.type.localeCompare(b.type) ||
      a.dy - b.dy ||
      a.dx - b.dx
    );

  const a = JSON.stringify({ rel: relBase, hands: initial.hands, sideToMove: initial.sideToMove });
  const b = JSON.stringify({ rel: relMirror, hands: initial.hands, sideToMove: initial.sideToMove });
  return a < b ? a : b;
}

/** Generate a random 3手詰 candidate with pieces close to the defender king */
function randomCandidate3(rand) {
  const used = new Set();
  const occ = (x, y) => used.has(`${x},${y}`);
  const put = (x, y) => used.add(`${x},${y}`);

  const pieces = [];
  // Place defender king near top edge (common tsume-shogi pattern)
  const dk = { x: ri(rand, 2, 8), y: ri(rand, 1, 3), owner: "defender", type: "K" };
  put(dk.x, dk.y);
  pieces.push(dk);

  // Choose 2-3 attacker pieces, placed near the defender king
  const atkCount = ri(rand, 2, 3);
  const atkTypes = ["R", "B", "G", "S", "P", "G", "S"];
  for (let i = 0; i < atkCount; i++) {
    const t = pick(rand, atkTypes);
    let x, y, g = 0;
    do {
      x = dk.x + ri(rand, -3, 3);
      y = dk.y + ri(rand, -2, 4);
      x = Math.max(1, Math.min(9, x));
      y = Math.max(1, Math.min(9, y));
      g++;
    } while (occ(x, y) && g < 40);
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "attacker", type: t });
  }

  // 0-2 defender pieces near their king
  const defCount = ri(rand, 0, 2);
  for (let i = 0; i < defCount; i++) {
    const t = pick(rand, ["G", "S", "P", "G", "S"]);
    let x, y, g = 0;
    do {
      x = dk.x + ri(rand, -2, 2);
      y = dk.y + ri(rand, -1, 2);
      x = Math.max(1, Math.min(9, x));
      y = Math.max(1, Math.min(9, y));
      g++;
    } while (occ(x, y) && g < 40);
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "defender", type: t });
  }

  // Occasionally give attacker a hand piece
  const hands = emptyHands();
  if (rand() < 0.3) hands.attacker[pick(rand, ["P", "S", "G"])] = 1;
  if (rand() < 0.1) hands.attacker[pick(rand, ["R", "B"])] = 1;

  return { pieces, hands, sideToMove: "attacker" };
}

/** Generate a random 5手詰 candidate */
function randomCandidate5(rand) {
  const used = new Set();
  const occ = (x, y) => used.has(`${x},${y}`);
  const put = (x, y) => used.add(`${x},${y}`);

  const pieces = [];
  const dk = { x: ri(rand, 2, 8), y: ri(rand, 1, 3), owner: "defender", type: "K" };
  put(dk.x, dk.y);
  pieces.push(dk);

  // 2-4 attacker pieces
  const atkCount = ri(rand, 2, 4);
  const atkTypes = ["R", "B", "G", "S", "P", "R", "G", "S"];
  for (let i = 0; i < atkCount; i++) {
    const t = pick(rand, atkTypes);
    let x, y, g = 0;
    do {
      x = dk.x + ri(rand, -4, 4);
      y = dk.y + ri(rand, -2, 5);
      x = Math.max(1, Math.min(9, x));
      y = Math.max(1, Math.min(9, y));
      g++;
    } while (occ(x, y) && g < 40);
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "attacker", type: t });
  }

  // 1-3 defender pieces
  const defCount = ri(rand, 1, 3);
  for (let i = 0; i < defCount; i++) {
    const t = pick(rand, ["G", "S", "P", "G", "S", "R"]);
    let x, y, g = 0;
    do {
      x = dk.x + ri(rand, -2, 2);
      y = dk.y + ri(rand, -1, 3);
      x = Math.max(1, Math.min(9, x));
      y = Math.max(1, Math.min(9, y));
      g++;
    } while (occ(x, y) && g < 40);
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "defender", type: t });
  }

  const hands = emptyHands();
  if (rand() < 0.4) hands.attacker[pick(rand, ["P", "S", "G"])] = 1;
  if (rand() < 0.15) hands.attacker[pick(rand, ["R", "B"])] = 1;

  return { pieces, hands, sideToMove: "attacker" };
}

function mutateInitial(rand, seed) {
  const cand = cloneInitial(seed);
  const ops = ["move-piece", "move-piece", "move-piece", "swap-type", "add-piece", "remove-piece", "tweak-hand"];
  const op = pick(rand, ops);

  if (op === "move-piece") {
    const movable = cand.pieces.filter((p) => p.type !== "K");
    if (movable.length === 0) return null;
    const target = pick(rand, movable);
    target.x += pick(rand, [-2, -1, 1, 2]);
    target.y += pick(rand, [-2, -1, 1, 2]);
  }

  if (op === "swap-type") {
    const movable = cand.pieces.filter((p) => p.type !== "K");
    if (movable.length === 0) return null;
    const target = pick(rand, movable);
    const types = target.owner === "attacker" ? ["R", "B", "G", "S", "P"] : ["G", "S", "P"];
    target.type = pick(rand, types);
  }

  if (op === "add-piece") {
    const owner = pick(rand, ["attacker", "defender"]);
    const type = owner === "attacker" ? pick(rand, ["R", "G", "S", "P", "B"]) : pick(rand, ["G", "S", "P"]);
    const dk = cand.pieces.find((p) => p.owner === "defender" && p.type === "K");
    if (dk) {
      const x = Math.max(1, Math.min(9, dk.x + ri(rand, -3, 3)));
      const y = Math.max(1, Math.min(9, dk.y + ri(rand, -2, 4)));
      cand.pieces.push({ x, y, owner, type });
    }
    if (cand.pieces.length > 10) {
      const removable = cand.pieces.filter((p) => p.type !== "K");
      if (removable.length > 0) {
        const rm = pick(rand, removable);
        cand.pieces = cand.pieces.filter((p) => p !== rm);
      }
    }
  }

  if (op === "remove-piece") {
    const removable = cand.pieces.filter((p) => p.type !== "K");
    if (removable.length > 2) {
      const rm = pick(rand, removable);
      cand.pieces = cand.pieces.filter((p) => p !== rm);
    }
  }

  if (op === "tweak-hand") {
    const t = pick(rand, ["P", "S", "G", "R", "B"]);
    cand.hands.attacker[t] = ri(rand, 0, 1);
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
  return atkTypes * 2 + dropCount * 5 + promoteCount * 3 + uniqueTargets * 2 - Math.max(0, pieceCount - 8);
}

function removePieceAt(initial, index) {
  return {
    pieces: initial.pieces.filter((_, i) => i !== index),
    hands: {
      attacker: { ...initial.hands.attacker },
      defender: { ...initial.hands.defender },
    },
    sideToMove: initial.sideToMove,
  };
}

function pruneInitial(initial, mateLength) {
  let cur = cloneInitial(initial);
  let changed = true;

  while (changed) {
    changed = false;
    const order = cur.pieces
      .map((p, i) => ({ p, i }))
      .filter(({ p }) => p.type !== "K")
      .sort((a, b) => {
        if (a.p.owner !== b.p.owner) return a.p.owner === "defender" ? -1 : 1;
        const ay = Math.abs(a.p.y - 5);
        const by = Math.abs(b.p.y - 5);
        return by - ay;
      });

    for (const { i } of order) {
      const cand = removePieceAt(cur, i);
      if (!basicValidity(cand)) continue;
      const r = validateTsumePuzzle(createState(cand), mateLength);
      if (!r.ok) continue;
      cur = cand;
      changed = true;
      break;
    }
  }

  return cur;
}

function validateAndPrune(initial, mateLength) {
  const normalized = stripAttackerKing(initial);
  const st = createState(normalized);
  const res = validateTsumePuzzle(st, mateLength);
  if (!res.ok) return null;

  const pruned = pruneInitial(normalized, mateLength);
  const prunedState = createState(pruned);
  const prunedRes = validateTsumePuzzle(prunedState, mateLength);
  const final = prunedRes && prunedRes.ok ? pruned : initial;
  const finalRes = prunedRes && prunedRes.ok ? prunedRes : res;
  const serial = toSerializable(createState(final));
  return { initial: serial, solution: finalRes.principalVariation, score: scorePuzzle(serial, finalRes.principalVariation) };
}

function generatePuzzles(rand, mateLength, attempts, curatedSeeds, max) {
  const sigSet = new Set();
  const structSet = new Set();
  const results = [];

  const addResult = (ok) => {
    const sig = JSON.stringify(ok.initial);
    const ssig = structuralSignature(ok.initial);
    if (sigSet.has(sig) || structSet.has(ssig)) return false;
    sigSet.add(sig);
    structSet.add(ssig);
    results.push(ok);
    return true;
  };

  // Add curated puzzles first
  for (const initial of curatedSeeds) {
    const ok = validateAndPrune(initial, mateLength);
    if (ok) addResult(ok);
  }

  // Also add mirror variants of curated
  for (const initial of curatedSeeds) {
    const mirrored = {
      pieces: initial.pieces.map((p) => ({ ...p, x: 10 - p.x })),
      hands: initial.hands,
      sideToMove: initial.sideToMove,
    };
    if (basicValidity(mirrored)) {
      const ok = validateAndPrune(mirrored, mateLength);
      if (ok) addResult(ok);
    }
  }

  const candidateFn = mateLength === 3 ? randomCandidate3 : randomCandidate5;

  // Phase 1: mutate from known good puzzles (if any)
  const mutateAttempts = Math.floor(attempts * 0.5);
  if (results.length > 0) {
    for (let i = 0; i < mutateAttempts && results.length < max; i++) {
      const seed = pick(rand, results);
      const cand = mutateInitial(rand, seed.initial);
      if (!cand) continue;
      const ok = validateAndPrune(cand, mateLength);
      if (ok) addResult(ok);
    }
  }

  // Phase 2: random generation
  const randomAttempts = attempts - mutateAttempts;
  for (let i = 0; i < randomAttempts && results.length < max; i++) {
    const cand = candidateFn(rand);
    if (!cand) continue;
    const ok = validateAndPrune(cand, mateLength);
    if (ok) {
      addResult(ok);
      // Also try mutations of freshly found puzzles
      for (let j = 0; j < 20 && results.length < max; j++) {
        const mutated = mutateInitial(rand, ok.initial);
        if (!mutated) continue;
        const mutOk = validateAndPrune(mutated, mateLength);
        if (mutOk) addResult(mutOk);
      }
      // Try mirror
      const mirrored = {
        pieces: ok.initial.pieces.map((p) => ({ ...p, x: 10 - p.x })),
        hands: ok.initial.hands,
        sideToMove: ok.initial.sideToMove,
      };
      if (basicValidity(mirrored)) {
        const mirOk = validateAndPrune(mirrored, mateLength);
        if (mirOk) addResult(mirOk);
      }
    }

    if (i % 2000 === 0 && i > 0) {
      console.error(`  ${mateLength}手詰: ${i}/${randomAttempts} attempts, ${results.length} found`);
    }
  }

  // Sort by score (best first) and assign IDs
  results.sort((a, b) => b.score - a.score);
  return results.slice(0, max).map((r, i) => ({
    id: i + 1,
    mateLength,
    initial: r.initial,
    solution: r.solution,
    quality: "validated",
    score: r.score,
  }));
}

function writeJson(file, data) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(data, null, 2), "utf-8");
}

function main() {
  const { maxPerCategory, seed, attempts3, attempts5 } = parseArgs();
  const rand = rng(seed);
  const curated = loadCurated();

  for (const n of [3, 5]) {
    const seeds = curated[String(n)] || [];
    const attempts = n === 3 ? attempts3 : attempts5;
    const puzzles = generatePuzzles(rand, n, attempts, seeds, maxPerCategory);

    writeJson(`puzzles/${n}.json`, puzzles);
    writeJson(`docs/puzzles/${n}.json`, puzzles);

    console.log(`${n}手詰: ${puzzles.length}問 (attempts=${attempts})`);
  }

  console.log(`seed=${seed}`);
}

main();
