// 将棋ロジック — Rust WASM バックエンド
//
// Rust (shogi-core crate) の WASM ビルドを呼び出す薄いラッパー。
// app.js との互換性を維持するため、State は {board: Map, hands, sideToMove} 形式。

import init, {
  createState as wasmCreateState,
  applyMoveW,
  legalMovesW,
  isInCheckW,
  findBestDefenseW,
  validateTsumePuzzleW,
  kingPosW,
} from "./wasm-pkg/shogi_wasm.js";

// --- WASM 初期化 ---

let _initPromise = null;

export function initWasm() {
  if (!_initPromise) {
    _initPromise = _doInit();
  }
  return _initPromise;
}

async function _doInit() {
  // Node.js 環境ではファイルから直接読み込む（ブラウザでは fetch を使う）
  if (typeof globalThis.process !== "undefined" && globalThis.process.versions?.node) {
    // 動的 import でバンドラーの静的解析を回避
    const fs = await import(/* @vite-ignore */ "node:fs");
    const url = await import(/* @vite-ignore */ "node:url");
    const path = await import(/* @vite-ignore */ "node:path");
    const dir = path.dirname(url.fileURLToPath(import.meta.url));
    const wasmPath = path.join(dir, "wasm-pkg", "shogi_wasm_bg.wasm");
    const wasmBytes = fs.readFileSync(wasmPath);
    const wasmModule = await WebAssembly.compile(wasmBytes);
    await init({ module_or_path: wasmModule });
  } else {
    await init();
  }
}

// --- 定数（純 JS — WASM 不要） ---

const PIECE_LABEL = {
  K: "玉", R: "飛", B: "角", G: "金", S: "銀", N: "桂", L: "香", P: "歩",
  "+R": "龍", "+B": "馬", "+S": "全", "+N": "圭", "+L": "杏", "+P": "と",
};

const HAND_TYPES = ["R", "B", "G", "S", "N", "L", "P"];

// --- State の変換ヘルパー ---

/** State (Map ベース) を WASM 用 JSON 文字列に変換 */
function stateToJson(state) {
  const boardObj = {};
  for (const [k, v] of state.board.entries()) {
    boardObj[k] = v;
  }
  return JSON.stringify({
    board: boardObj,
    hands: state.hands,
    sideToMove: state.sideToMove,
  });
}

/** WASM から返された JSON 文字列を State (Map ベース) に変換 */
function jsonToState(json) {
  const raw = JSON.parse(json);
  const board = new Map();
  for (const [k, v] of Object.entries(raw.board)) {
    board.set(k, v);
  }
  return {
    board,
    hands: raw.hands,
    sideToMove: raw.sideToMove,
  };
}

// --- 公開 API ---

export function emptyHands() {
  const h = { attacker: {}, defender: {} };
  for (const t of HAND_TYPES) {
    h.attacker[t] = 0;
    h.defender[t] = 0;
  }
  return h;
}

export function createState({ pieces, hands, sideToMove }) {
  const initial = { pieces, hands: hands || emptyHands(), sideToMove: sideToMove || "attacker" };
  const json = wasmCreateState(JSON.stringify(initial));
  return jsonToState(json);
}

export function cloneState(state) {
  const board = new Map(state.board);
  return {
    board,
    hands: {
      attacker: { ...state.hands.attacker },
      defender: { ...state.hands.defender },
    },
    sideToMove: state.sideToMove,
  };
}

export function toSerializable(state) {
  const pieces = [];
  for (const [k, p] of state.board.entries()) {
    const [x, y] = k.split(",").map(Number);
    pieces.push({ x, y, owner: p.owner, type: p.type });
  }
  pieces.sort((a, b) => a.y - b.y || a.x - b.x || a.owner.localeCompare(b.owner) || a.type.localeCompare(b.type));
  return {
    pieces,
    hands: {
      attacker: { ...state.hands.attacker },
      defender: { ...state.hands.defender },
    },
    sideToMove: state.sideToMove,
  };
}

export function applyMove(state, move) {
  const sj = stateToJson(state);
  const mj = JSON.stringify(move);
  return jsonToState(applyMoveW(sj, mj));
}

export function legalMoves(state) {
  const sj = stateToJson(state);
  return JSON.parse(legalMovesW(sj));
}

export function isInCheck(state, owner) {
  const sj = stateToJson(state);
  return isInCheckW(sj, owner);
}

export function findBestDefense(state, remainingPlies) {
  const sj = stateToJson(state);
  const result = findBestDefenseW(sj, remainingPlies);
  return JSON.parse(result);
}

export function validateTsumePuzzle(state, mateLength) {
  const sj = stateToJson(state);
  const result = validateTsumePuzzleW(sj, mateLength);
  const pv = JSON.parse(result);
  if (pv === null) {
    return { ok: false, reason: "no unique solution" };
  }
  return { ok: true, reason: "ok", principalVariation: pv };
}

// --- 純 JS ヘルパー（WASM 不要） ---

export function formatMove(move) {
  if (move.drop) {
    return `${PIECE_LABEL[move.drop]}打(${move.to[0]},${move.to[1]})`;
  }
  return `(${move.from[0]},${move.from[1]})→(${move.to[0]},${move.to[1]})${move.promote ? "成" : ""}`;
}

export function pieceToText(piece) {
  const symbol = PIECE_LABEL[piece.type] || piece.type;
  return piece.owner === "attacker" ? symbol : `v${symbol}`;
}

export function normalizeMove(move) {
  return {
    from: move.from ? [move.from[0], move.from[1]] : undefined,
    to: [move.to[0], move.to[1]],
    drop: move.drop || undefined,
    promote: !!move.promote,
  };
}

export function sameMove(a, b) {
  if (!!a.drop !== !!b.drop) return false;
  if (a.drop) {
    return a.drop === b.drop && a.to[0] === b.to[0] && a.to[1] === b.to[1];
  }
  return (
    a.from[0] === b.from[0] &&
    a.from[1] === b.from[1] &&
    a.to[0] === b.to[0] &&
    a.to[1] === b.to[1] &&
    !!a.promote === !!b.promote
  );
}
