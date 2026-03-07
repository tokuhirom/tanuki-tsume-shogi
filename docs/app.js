import {
  applyMove,
  cloneState,
  createState,
  formatMove,
  normalizeMove,
  pieceToText,
  sameMove,
} from "./shogi-core.js";

const PIECE_LABEL = {
  K: "玉", R: "飛", B: "角", G: "金", S: "銀", P: "歩",
  "+R": "龍", "+B": "馬", "+S": "全", "+P": "と",
};

const app = document.getElementById("app");
const lengths = [3, 5];

const storeKey = (len, id) => `tanuki-tsume:v1:clear:${len}:${id}`;
const isCleared = (len, id) => localStorage.getItem(storeKey(len, id)) === "true";
const markClear = (len, id) => localStorage.setItem(storeKey(len, id), "true");
const soundEnabledKey = "tanuki-tsume:v1:sound-enabled";
const isSoundEnabled = () => localStorage.getItem(soundEnabledKey) !== "false";
const setSoundEnabled = (v) => localStorage.setItem(soundEnabledKey, v ? "true" : "false");

const state = {
  screen: "title",
  mateLength: null,
  puzzles: [],
  puzzle: null,
  gameState: null,
  ply: 0,
  selectedSquare: null,
  selectedHand: null,
  message: "",
  clearFxUntil: 0,
  soundEnabled: isSoundEnabled(),
  history: [],
  promotionPrompt: null,
  lastMove: null,
  showSolution: false,
  buildInfo: { branch: "unknown", commit: "unknown", builtAt: "unknown" },
};

function parseRoute() {
  const params = new URLSearchParams(window.location.search);
  const mate = Number(params.get("mate"));
  const id = Number(params.get("id"));
  if (!lengths.includes(mate)) return { screen: "title" };
  if (!Number.isInteger(id) || id <= 0) return { screen: "list", mateLength: mate };
  return { screen: "puzzle", mateLength: mate, puzzleId: id };
}

function setRoute({ mateLength = null, puzzleId = null } = {}) {
  const url = new URL(window.location.href);
  url.searchParams.delete("mate");
  url.searchParams.delete("id");
  if (mateLength) url.searchParams.set("mate", String(mateLength));
  if (puzzleId) url.searchParams.set("id", String(puzzleId));
  window.history.replaceState({}, "", `${url.pathname}${url.search}`);
}

let audioCtx = null;

function ensureAudio() {
  if (!audioCtx) {
    audioCtx = new (window.AudioContext || window.webkitAudioContext)();
  }
  if (audioCtx.state === "suspended") {
    audioCtx.resume();
  }
  return audioCtx;
}

function tone({ freq, start, duration, volume = 0.05, type = "sine" }) {
  const ctx = ensureAudio();
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.type = type;
  osc.frequency.setValueAtTime(freq, start);
  gain.gain.setValueAtTime(0.0001, start);
  gain.gain.exponentialRampToValueAtTime(volume, start + 0.01);
  gain.gain.exponentialRampToValueAtTime(0.0001, start + duration);
  osc.connect(gain);
  gain.connect(ctx.destination);
  osc.start(start);
  osc.stop(start + duration + 0.02);
}

function playMoveSound() {
  if (!state.soundEnabled) return;
  const ctx = ensureAudio();
  const now = ctx.currentTime;
  tone({ freq: 740, start: now, duration: 0.06, volume: 0.035, type: "triangle" });
  tone({ freq: 920, start: now + 0.04, duration: 0.07, volume: 0.03, type: "triangle" });
}

function playClearSound() {
  if (!state.soundEnabled) return;
  const ctx = ensureAudio();
  const now = ctx.currentTime;
  tone({ freq: 660, start: now, duration: 0.09, volume: 0.05, type: "triangle" });
  tone({ freq: 880, start: now + 0.08, duration: 0.12, volume: 0.05, type: "triangle" });
  tone({ freq: 1320, start: now + 0.18, duration: 0.2, volume: 0.06, type: "sine" });
}

function toggleSound() {
  state.soundEnabled = !state.soundEnabled;
  setSoundEnabled(state.soundEnabled);
  state.message = state.soundEnabled ? "音声をオンにしました。" : "音声をオフにしました。";
  render();
}

function soundToggleButton() {
  return h(
    "button",
    { class: "btn small", onclick: toggleSound },
    state.soundEnabled ? "🔊 音: ON" : "🔈 音: OFF",
  );
}

function h(tag, attrs = {}, children = []) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k.startsWith("on") && typeof v === "function") {
      el.addEventListener(k.slice(2).toLowerCase(), v);
    } else if (k === "class") {
      el.className = v;
    } else {
      el.setAttribute(k, v);
    }
  }
  for (const c of Array.isArray(children) ? children : [children]) {
    if (c == null) continue;
    el.append(c.nodeType ? c : document.createTextNode(String(c)));
  }
  return el;
}

async function loadPuzzles(len) {
  const res = await fetch(`./puzzles/${len}.json?v=20260307b`);
  if (!res.ok) throw new Error("問題データの読み込みに失敗しました");
  return res.json();
}

async function loadBuildInfo() {
  try {
    const res = await fetch("./build-info.json?v=20260307b");
    if (!res.ok) return;
    const json = await res.json();
    state.buildInfo = {
      branch: json.branch || "unknown",
      commit: json.commit || "unknown",
      builtAt: json.builtAt || "unknown",
    };
  } catch {
    // ignore
  }
}

function goTitle() {
  state.screen = "title";
  setRoute();
  render();
}

async function goList(len) {
  state.mateLength = len;
  state.puzzles = await loadPuzzles(len);
  state.screen = "list";
  setRoute({ mateLength: len });
  render();
}

function goPuzzle(p) {
  state.puzzle = p;
  state.gameState = createState(p.initial);
  state.ply = 0;
  state.history = [];
  state.selectedSquare = null;
  state.selectedHand = null;
  state.promotionPrompt = null;
  state.lastMove = null;
  state.showSolution = false;
  state.message = "攻め方の手を選んでください";
  state.screen = "puzzle";
  setRoute({ mateLength: state.mateLength, puzzleId: p.id });
  render();
}

function goNextPuzzle() {
  if (!state.puzzle || !state.puzzles.length) return;
  const idx = state.puzzles.findIndex((p) => p.id === state.puzzle.id);
  const next = state.puzzles[idx + 1];
  if (next) {
    goPuzzle(next);
  } else {
    goList(state.mateLength);
  }
}

function goPrevPuzzle() {
  if (!state.puzzle || !state.puzzles.length) return;
  const idx = state.puzzles.findIndex((p) => p.id === state.puzzle.id);
  const prev = state.puzzles[idx - 1];
  if (prev) {
    goPuzzle(prev);
  }
}

function isPuzzleCleared() {
  return state.ply >= state.puzzle.solution.length;
}

function currentExpectedMove() {
  return normalizeMove(state.puzzle.solution[state.ply]);
}

function getMoveTargets() {
  if (!state.selectedSquare && !state.selectedHand) return new Set();
  const expected = currentExpectedMove();
  const targets = new Set();

  if (state.selectedHand) {
    if (expected.drop === state.selectedHand) {
      targets.add(`${expected.to[0]},${expected.to[1]}`);
    }
  } else if (state.selectedSquare) {
    const [sx, sy] = state.selectedSquare;
    if (expected.from && expected.from[0] === sx && expected.from[1] === sy) {
      targets.add(`${expected.to[0]},${expected.to[1]}`);
    }
  }
  return targets;
}

function tryUserMove(candidate) {
  if (!state.puzzle || !state.gameState) return false;
  const expected = currentExpectedMove();
  if (!sameMove(candidate, expected)) {
    return false;
  }

  state.history.push({
    gameState: cloneState(state.gameState),
    ply: state.ply,
    message: state.message,
    lastMove: state.lastMove,
  });

  state.gameState = applyMove(state.gameState, candidate);
  state.lastMove = candidate;
  playMoveSound();
  state.ply += 1;
  state.selectedSquare = null;
  state.selectedHand = null;

  while (state.ply < state.puzzle.solution.length && state.gameState.sideToMove === "defender") {
    const auto = normalizeMove(state.puzzle.solution[state.ply]);
    state.gameState = applyMove(state.gameState, auto);
    state.lastMove = auto;
    playMoveSound();
    state.ply += 1;
  }

  if (state.ply >= state.puzzle.solution.length) {
    markClear(state.mateLength, state.puzzle.id);
    state.message = "クリア！";
    state.clearFxUntil = Date.now() + 1500;
    playClearSound();
  } else {
    state.message = "正解！ 次の一手へ。";
  }
  render();
  return true;
}

function undoOneTurn() {
  if (state.history.length === 0) return;
  const prev = state.history.pop();
  state.gameState = prev.gameState;
  state.ply = prev.ply;
  state.lastMove = prev.lastMove;
  state.selectedSquare = null;
  state.selectedHand = null;
  state.promotionPrompt = null;
  state.clearFxUntil = 0;
  state.message = "一手戻しました。";
  render();
}

function boardPiece(x, y) {
  return state.gameState.board.get(`${x},${y}`) || null;
}

function isHiddenAttackerKing(piece) {
  return piece && piece.owner === "attacker" && piece.type === "K";
}

function boardViewport() {
  const points = [];
  for (const [k, p] of state.gameState.board.entries()) {
    if (isHiddenAttackerKing(p)) continue;
    const [x, y] = k.split(",").map(Number);
    points.push({ x, y });
  }
  if (points.length === 0) {
    return { minX: 1, maxX: 9, minY: 1, maxY: 9 };
  }

  let minX = Math.min(...points.map((p) => p.x));
  let maxX = Math.max(...points.map((p) => p.x));
  let minY = Math.min(...points.map((p) => p.y));
  let maxY = Math.max(...points.map((p) => p.y));

  minX = Math.max(1, minX - 1);
  maxX = Math.min(9, maxX + 1);
  minY = Math.max(1, minY - 1);
  maxY = Math.min(9, maxY + 1);

  while (maxX - minX + 1 < 5) {
    if (minX > 1) minX -= 1;
    else if (maxX < 9) maxX += 1;
    else break;
  }
  while (maxY - minY + 1 < 5) {
    if (minY > 1) minY -= 1;
    else if (maxY < 9) maxY += 1;
    else break;
  }

  return { minX, maxX, minY, maxY };
}

function onSquareClick(x, y) {
  if (state.gameState.sideToMove !== "attacker") return;
  if (state.promotionPrompt) return;
  if (isPuzzleCleared()) return;
  const target = boardPiece(x, y);

  if (state.selectedHand) {
    if (!tryUserMove({ drop: state.selectedHand, to: [x, y], promote: false })) {
      state.selectedHand = null;
      state.message = "そこには打てません。";
      render();
    }
    return;
  }

  if (!state.selectedSquare) {
    if (target && target.owner === "attacker" && target.type !== "K") {
      state.selectedSquare = [x, y];
      state.message = "移動先を選んでください";
      render();
    }
    return;
  }

  const [fx, fy] = state.selectedSquare;
  if (fx === x && fy === y) {
    state.selectedSquare = null;
    state.message = "攻め方の手を選んでください";
    render();
    return;
  }

  if (target && target.owner === "attacker" && target.type !== "K") {
    state.selectedSquare = [x, y];
    state.message = "移動先を選んでください";
    render();
    return;
  }

  const moving = boardPiece(fx, fy);
  if (!moving) return;
  const moveBase = { from: [fx, fy], to: [x, y], promote: false };
  const promotable = new Set(["R", "B", "S", "P"]);
  const inZone = (yy) => yy <= 3;
  const canPromote =
    moving.owner === "attacker" &&
    promotable.has(moving.type) &&
    !moving.type.startsWith("+") &&
    (inZone(fy) || inZone(y));

  if (!canPromote) {
    if (!tryUserMove(moveBase)) {
      state.selectedSquare = null;
      state.message = "その手は正解ではありません。";
      render();
    }
    return;
  }
  if (moving.type === "P" && y === 1) {
    if (!tryUserMove({ ...moveBase, promote: true })) {
      state.selectedSquare = null;
      state.message = "その手は正解ではありません。";
      render();
    }
    return;
  }
  state.promotionPrompt = moveBase;
  render();
}

function choosePromotion(promote) {
  if (!state.promotionPrompt) return;
  const base = state.promotionPrompt;
  const move = { ...base, promote };
  state.promotionPrompt = null;
  if (tryUserMove(move)) return;

  const alt = { ...base, promote: !promote };
  if (tryUserMove(alt)) return;

  state.promotionPrompt = base;
  render();
}

async function copyPuzzleLink() {
  if (!state.puzzle || !state.mateLength) return;
  const url = new URL(window.location.href);
  url.searchParams.set("mate", String(state.mateLength));
  url.searchParams.set("id", String(state.puzzle.id));
  const text = url.toString();
  try {
    await navigator.clipboard.writeText(text);
    state.message = "リンクをコピーしました。";
  } catch {
    state.message = `リンク: ${text}`;
  }
  render();
}

function renderTitle() {
  const bi = state.buildInfo;
  return h("section", { class: "panel" }, [
    h("div", { class: "top-hero" }, [
      h("div", {}, [
        h("h1", {}, "たぬき詰将棋"),
        h("p", {}, "タヌキと一緒に、3手詰・5手詰をサクサク挑戦！"),
        h("div", { class: "grid4" }, lengths.map((n) =>
          h("button", { class: "btn primary", onclick: () => goList(n) }, `${n}手詰へ`)
        )),
      ]),
      h("img", { src: "./assets/tanuki.svg", alt: "タヌキ" }),
    ]),
    h("div", { class: "toolbar" }, [
      soundToggleButton(),
    ]),
    h("div", { class: "build-info" }, `${bi.branch} / ${bi.commit.slice(0, 7)} / ${bi.builtAt}`),
    h("div", { class: "app-footer" }, [
      h("a", {
        class: "footer-link",
        href: "https://github.com/tokuhirom/tanuki-tsume-shogi",
        target: "_blank",
        rel: "noopener noreferrer",
      }, "GitHub"),
    ]),
  ]);
}

function renderList() {
  const hasPuzzles = state.puzzles.length > 0;
  const cleared = state.puzzles.filter((p) => isCleared(state.mateLength, p.id)).length;
  return h("section", { class: "panel" }, [
    h("div", { class: "toolbar" }, [
      h("button", { class: "btn small", onclick: goTitle }, "← タイトル"),
      h("span", { class: "spacer" }),
      soundToggleButton(),
    ]),
    h("h2", {}, `${state.mateLength}手詰`),
    h("p", {}, `クリア: ${cleared} / ${state.puzzles.length}`),
    hasPuzzles
      ? h("div", { class: "puzzle-grid" }, state.puzzles.map((p) =>
          h("button", {
            class: `puzzle-num${isCleared(state.mateLength, p.id) ? " clear" : ""}`,
            onclick: () => goPuzzle(p),
          }, p.id)
        ))
      : h("p", { class: "log" }, "この手数カテゴリは検証済み問題を準備中です。"),
  ]);
}

function renderHands() {
  const hands = state.gameState.hands.attacker;
  const pieces = Object.entries(hands).filter(([, c]) => c > 0);
  if (pieces.length === 0) return null;
  return h("div", { class: "hand-area" }, [
    h("div", { class: "hand-label" }, "持ち駒"),
    h("div", { class: "row" }, pieces.map(([piece, count]) =>
      h("button", {
        class: `btn hand-btn${state.selectedHand === piece ? " primary" : ""}`,
        onclick: () => {
          if (isPuzzleCleared()) return;
          state.selectedHand = state.selectedHand === piece ? null : piece;
          state.selectedSquare = null;
          state.message = state.selectedHand ? `${PIECE_LABEL[piece]}を打つ場所を選んでください` : "攻め方の手を選んでください";
          render();
        },
      }, `${PIECE_LABEL[piece] || piece} ×${count}`)
    )),
  ]);
}

function renderBoard() {
  const view = boardViewport();
  const table = h("table", { class: "board" });
  const kanji = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
  const fileLabel = (x) => String(x);
  const targets = isPuzzleCleared() ? new Set() : getMoveTargets();
  const lm = state.lastMove;

  const head = h("tr");
  head.append(h("th", { class: "coord corner" }, " "));
  for (let x = view.minX; x <= view.maxX; x += 1) {
    head.append(h("th", { class: "coord file" }, fileLabel(x)));
  }
  head.append(h("th", { class: "coord corner" }, " "));
  table.append(head);

  for (let y = view.minY; y <= view.maxY; y += 1) {
    const tr = h("tr");
    tr.append(h("th", { class: "coord corner" }, " "));
    for (let x = view.minX; x <= view.maxX; x += 1) {
      const p = boardPiece(x, y);
      const selected = state.selectedSquare && state.selectedSquare[0] === x && state.selectedSquare[1] === y;
      const isTarget = targets.has(`${x},${y}`);
      const isLastMove = lm && lm.to[0] === x && lm.to[1] === y;
      const text = p && !isHiddenAttackerKing(p) ? pieceToText(p).replace(/^v/, "") : "";
      const isPromoted = p && p.type.startsWith("+");
      const pieceNode = text
        ? h("span", { class: `piece${p.owner === "defender" ? " defender" : ""}${isPromoted ? " promoted" : ""}` }, text)
        : "";
      const edgeTop = y === 1 ? " edge-top" : "";
      const edgeBottom = y === 9 ? " edge-bottom" : "";
      const edgeLeft = x === 1 ? " edge-left" : "";
      const edgeRight = x === 9 ? " edge-right" : "";
      const classes = [
        selected ? "sel" : "",
        isTarget ? "move-target" : "",
        isLastMove && !selected ? "last-move" : "",
        edgeTop, edgeBottom, edgeLeft, edgeRight,
      ].filter(Boolean).join(" ");
      tr.append(h("td", {}, h("button", {
        class: classes,
        "data-x": String(x),
        "data-y": String(y),
        onclick: () => onSquareClick(x, y),
      }, pieceNode)));
    }
    tr.append(h("th", { class: "coord rank" }, kanji[y]));
    table.append(tr);
  }

  return table;
}

function renderSolutionToggle() {
  if (!state.showSolution) {
    return h("div", { class: "solution-toggle", onclick: () => { state.showSolution = true; render(); } }, "▶ 手順を表示");
  }
  const list = state.puzzle.solution.map((m, i) => `${i + 1}. ${formatMove(m)}`);
  return h("div", { class: "log" }, [
    h("div", { class: "solution-toggle", onclick: () => { state.showSolution = false; render(); } }, "▼ 手順を隠す"),
    h("div", {}, list.join(" / ")),
  ]);
}

function renderPuzzle() {
  const isClearFx = Date.now() < state.clearFxUntil;
  const cleared = isPuzzleCleared();
  const fxNodes = isClearFx
    ? h("div", { class: "fx-sparkles" }, Array.from({ length: 12 }).map((_, i) =>
      h("span", { style: `--i:${i}` }, "◆")
    ))
    : null;

  const hasPrev = state.puzzles.findIndex((p) => p.id === state.puzzle.id) > 0;
  const hasNext = state.puzzles.findIndex((p) => p.id === state.puzzle.id) < state.puzzles.length - 1;

  return h("section", { class: "panel" }, [
    h("div", { class: `puzzle-panel${isClearFx ? " victory" : ""}` }, [
      fxNodes,
      h("div", { class: "toolbar" }, [
        h("button", { class: "btn small", onclick: () => goList(state.mateLength) }, "← 一覧"),
        hasPrev ? h("button", { class: "btn small", onclick: goPrevPuzzle }, "◀ 前") : null,
        hasNext ? h("button", { class: "btn small", onclick: goNextPuzzle }, "次 ▶") : null,
        h("span", { class: "spacer" }),
        soundToggleButton(),
      ]),
      h("h2", {}, `${state.mateLength}手詰 #${state.puzzle.id}`),
      cleared ? h("div", { class: "clear-badge" }, "CLEAR!") : null,
      h("div", { class: "message" }, state.message),
      renderHands(),
      h("div", { class: "board-wrap" }, renderBoard()),
      state.promotionPrompt
        ? h("div", { class: "log" }, [
            h("div", {}, "成りますか？"),
            h("div", { class: "row" }, [
              h("button", { class: "btn primary", onclick: () => choosePromotion(true) }, "成る"),
              h("button", { class: "btn", onclick: () => choosePromotion(false) }, "成らない"),
            ]),
          ])
        : null,
      h("div", { class: "toolbar" }, [
        !cleared ? h("button", { class: "btn small", onclick: undoOneTurn }, "↩ 一手戻す") : null,
        h("button", { class: "btn small", onclick: copyPuzzleLink }, "🔗 リンク"),
        cleared && hasNext ? h("button", { class: "btn primary", onclick: goNextPuzzle }, "次の問題へ →") : null,
      ]),
      renderSolutionToggle(),
    ]),
  ]);
}

function render() {
  app.innerHTML = "";
  if (state.screen === "title") app.append(renderTitle());
  if (state.screen === "list") app.append(renderList());
  if (state.screen === "puzzle") app.append(renderPuzzle());
}

async function boot() {
  await loadBuildInfo();
  const route = parseRoute();
  if (route.screen === "title") {
    goTitle();
    return;
  }
  if (route.screen === "list") {
    await goList(route.mateLength);
    return;
  }

  await goList(route.mateLength);
  const target = state.puzzles.find((p) => p.id === route.puzzleId);
  if (target) {
    goPuzzle(target);
  } else {
    state.message = "指定された問題が見つかりませんでした。";
    render();
  }
}

boot();
