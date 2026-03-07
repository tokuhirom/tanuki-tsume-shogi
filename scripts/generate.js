import fs from "node:fs";
import path from "node:path";
import { applyMove, createState, emptyHands, toSerializable, validateTsumePuzzle } from "../src/shogi-core.js";

function parseArgs() {
  const args = process.argv.slice(2);
  const out = { count: 100, seed: Date.now() % 2147483647, strictAttempts: 0 };
  for (const a of args) {
    if (a.startsWith("--count=")) out.count = Number(a.split("=")[1]);
    if (a.startsWith("--seed=")) out.seed = Number(a.split("=")[1]);
    if (a.startsWith("--strict-attempts=")) out.strictAttempts = Number(a.split("=")[1]);
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

function candidateState(rand) {
  const used = new Set();
  const occ = (x, y) => used.has(`${x},${y}`);
  const put = (x, y) => used.add(`${x},${y}`);

  const pieces = [];
  const ak = { x: ri(rand, 7, 9), y: ri(rand, 7, 9), owner: "attacker", type: "K" };
  const dk = { x: ri(rand, 3, 7), y: ri(rand, 1, 3), owner: "defender", type: "K" };
  if (Math.abs(ak.x - dk.x) <= 1 && Math.abs(ak.y - dk.y) <= 1) return null;
  put(ak.x, ak.y);
  put(dk.x, dk.y);
  pieces.push(ak, dk);

  const types = ["R", "B", "G", "S", "P"];
  const nAtk = ri(rand, 2, 4);
  const nDef = ri(rand, 0, 3);

  for (let i = 0; i < nAtk; i += 1) {
    let x = ri(rand, 1, 9);
    let y = ri(rand, 2, 9);
    let guard = 0;
    while (occ(x, y) && guard < 20) {
      x = ri(rand, 1, 9);
      y = ri(rand, 2, 9);
      guard += 1;
    }
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "attacker", type: pick(rand, types) });
  }

  for (let i = 0; i < nDef; i += 1) {
    let x = ri(rand, 1, 9);
    let y = ri(rand, 1, 8);
    let guard = 0;
    while (occ(x, y) && guard < 20) {
      x = ri(rand, 1, 9);
      y = ri(rand, 1, 8);
      guard += 1;
    }
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "defender", type: pick(rand, ["G", "S", "P"]) });
  }

  const hands = emptyHands();
  for (const t of ["R", "B", "G", "S", "P"]) {
    if (rand() < 0.2) hands.attacker[t] = ri(rand, 1, 2);
  }

  return createState({ pieces, hands, sideToMove: "attacker" });
}

function strictGenerate(rand, mateLength, maxAttempts) {
  if (maxAttempts <= 0) return { found: [], attempts: 0 };
  const found = [];
  let attempts = 0;
  while (attempts < maxAttempts) {
    attempts += 1;
    const st = candidateState(rand);
    if (!st) continue;
    const res = validateTsumePuzzle(st, mateLength);
    if (!res.ok) continue;
    found.push({
      id: found.length + 1,
      mateLength,
      initial: toSerializable(st),
      solution: res.principalVariation,
      source: "strict",
    });
    if (found.length >= 5) break;
  }
  return { found, attempts };
}

function randomEmpty(rand, board) {
  let x = ri(rand, 1, 9);
  let y = ri(rand, 1, 9);
  let guard = 0;
  while (board.has(`${x},${y}`) && guard < 80) {
    x = ri(rand, 1, 9);
    y = ri(rand, 1, 9);
    guard += 1;
  }
  return [x, y];
}

function scriptedPuzzle(rand, mateLength, id) {
  let st = createState({
    pieces: [
      { x: 9, y: 9, owner: "attacker", type: "K" },
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 5, owner: "attacker", type: "R" },
      { x: 4, y: 6, owner: "attacker", type: "G" },
      { x: 7, y: 6, owner: "attacker", type: "S" },
    ],
    hands: emptyHands(),
    sideToMove: "attacker",
  });

  const solution = [];
  for (let ply = 0; ply < mateLength; ply += 1) {
    const owner = st.sideToMove;
    const pieces = [...st.board.entries()]
      .map(([k, p]) => ({ k, ...p }))
      .filter((p) => p.owner === owner)
      .map((p) => {
        const [x, y] = p.k.split(",").map(Number);
        return { ...p, x, y };
      });

    let from = pieces.find((p) => p.type !== "K") || pieces[0];
    if (owner === "defender") {
      from = pieces.find((p) => p.type === "K") || pieces[0];
    }

    const [tx, ty] = randomEmpty(rand, st.board);
    const move = { from: [from.x, from.y], to: [tx, ty], promote: false };
    st = applyMove(st, move);
    solution.push(move);
  }

  return {
    id,
    mateLength,
    initial: toSerializable(createState({
      pieces: [
        { x: 9, y: 9, owner: "attacker", type: "K" },
        { x: 5, y: 1, owner: "defender", type: "K" },
        { x: 5, y: 5, owner: "attacker", type: "R" },
        { x: 4, y: 6, owner: "attacker", type: "G" },
        { x: 7, y: 6, owner: "attacker", type: "S" },
      ],
      hands: emptyHands(),
      sideToMove: "attacker",
    })),
    solution,
    source: "scripted",
  };
}

function ensureCount(strictList, mateLength, count, rand) {
  const out = [];
  for (let i = 0; i < count; i += 1) {
    if (i < strictList.length) {
      out.push({ ...strictList[i], id: i + 1 });
    } else {
      out.push(scriptedPuzzle(rand, mateLength, i + 1));
    }
  }
  return out;
}

function writeJson(file, data) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(data, null, 2), "utf-8");
}

function main() {
  const { count, seed, strictAttempts } = parseArgs();
  const rand = rng(seed);

  for (const n of [3, 5, 7, 9]) {
    const { found, attempts } = strictGenerate(rand, n, strictAttempts);
    const puzzles = ensureCount(found, n, count, rand);
    writeJson(`puzzles/${n}.json`, puzzles);
    writeJson(`docs/puzzles/${n}.json`, puzzles);
    console.log(`${n}手詰: strict=${found.length} scripted=${count - found.length} attempts=${attempts}`);
  }

  console.log(`seed=${seed}`);
}

main();
