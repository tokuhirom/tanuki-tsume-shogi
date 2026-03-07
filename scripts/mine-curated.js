import {
  applyMove,
  createState,
  emptyHands,
  isInCheck,
  legalMoves,
  toSerializable,
  validateTsumePuzzle,
} from "../src/shogi-core.js";

function parseArgs() {
  const args = process.argv.slice(2);
  const out = {
    mateLength: 7,
    tries: 30000,
    seed: Date.now() % 2147483647,
  };
  for (const a of args) {
    if (a.startsWith("--mate=")) out.mateLength = Number(a.split("=")[1]);
    if (a.startsWith("--tries=")) out.tries = Number(a.split("=")[1]);
    if (a.startsWith("--seed=")) out.seed = Number(a.split("=")[1]);
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
  for (const t of atkTypes) {
    let x = ri(rand, Math.max(1, dk.x - 3), Math.min(9, dk.x + 3));
    let y = ri(rand, 2, 8);
    let guard = 0;
    while (occ(x, y) && guard < 30) {
      x = ri(rand, Math.max(1, dk.x - 3), Math.min(9, dk.x + 3));
      y = ri(rand, 2, 8);
      guard += 1;
    }
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "attacker", type: t });
  }

  const defenderCount = len >= 9 ? 3 : 2;
  for (let i = 0; i < defenderCount; i += 1) {
    const t = pick(rand, ["G", "S", "P"]);
    let x = ri(rand, Math.max(1, dk.x - 2), Math.min(9, dk.x + 2));
    let y = ri(rand, 1, 5);
    let guard = 0;
    while (occ(x, y) && guard < 30) {
      x = ri(rand, Math.max(1, dk.x - 2), Math.min(9, dk.x + 2));
      y = ri(rand, 1, 5);
      guard += 1;
    }
    if (occ(x, y)) continue;
    put(x, y);
    pieces.push({ x, y, owner: "defender", type: t });
  }

  const hands = emptyHands();
  if (len >= 7 && rand() < 0.45) hands.attacker.P = 1;
  if (len >= 7 && rand() < 0.35) hands.attacker.S = 1;
  if (len >= 9 && rand() < 0.25) hands.attacker.G = 1;

  return { pieces, hands, sideToMove: "attacker" };
}

function main() {
  const { mateLength, tries, seed } = parseArgs();
  const rand = rng(seed);
  let filtered = 0;

  for (let i = 1; i <= tries; i += 1) {
    const candidate = randomCandidate(rand, mateLength);
    if (!candidate) continue;
    const st = createState(candidate);

    // Fast pre-filter before expensive exact validation.
    const moves = legalMoves(st);
    if (moves.length === 0 || moves.length > 24) continue;
    const checks = moves.filter((m) => isInCheck(applyMove(st, m), "defender"));
    if (checks.length === 0 || checks.length > 8) continue;
    let immediateMate = false;
    for (const m of checks) {
      const next = applyMove(st, m);
      if (legalMoves(next).length === 0) {
        immediateMate = true;
        break;
      }
    }
    if (immediateMate) continue;
    filtered += 1;

    const result = validateTsumePuzzle(st, mateLength);
    if (result.ok) {
      console.log(JSON.stringify({
        mateLength,
        tries: i,
        seed,
        filtered,
        initial: toSerializable(st),
        principalVariation: result.principalVariation,
      }, null, 2));
      return;
    }

    if (i % 1000 === 0) {
      console.error(`progress: mate=${mateLength} tries=${i} filtered=${filtered}`);
    }
  }

  process.exit(2);
}

main();
