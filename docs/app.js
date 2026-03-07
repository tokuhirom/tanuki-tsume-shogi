import {
  applyMove,
  cloneState,
  createState,
  formatMove,
  normalizeMove,
  pieceToText,
  sameMove,
} from "./shogi-core.js";

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
    { class: "btn", onclick: toggleSound },
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
  state.message = "攻め方の手を選んでください";
  state.screen = "puzzle";
  setRoute({ mateLength: state.mateLength, puzzleId: p.id });
  render();
}

function currentExpectedMove() {
  return normalizeMove(state.puzzle.solution[state.ply]);
}

function tryUserMove(candidate) {
  if (!state.puzzle || !state.gameState) return;
  const expected = currentExpectedMove();
  if (!sameMove(candidate, expected)) {
    // Mistakes are ignored quietly to keep puzzle flow smooth.
    return false;
  }

  state.history.push({
    gameState: cloneState(state.gameState),
    ply: state.ply,
    message: state.message,
  });

  state.gameState = applyMove(state.gameState, candidate);
  playMoveSound();
  state.ply += 1;
  state.selectedSquare = null;
  state.selectedHand = null;

  while (state.ply < state.puzzle.solution.length && state.gameState.sideToMove === "defender") {
    const auto = normalizeMove(state.puzzle.solution[state.ply]);
    state.gameState = applyMove(state.gameState, auto);
    playMoveSound();
    state.ply += 1;
  }

  if (state.ply >= state.puzzle.solution.length) {
    markClear(state.mateLength, state.puzzle.id);
    state.message = "クリア！ localStorage に記録しました。";
    state.clearFxUntil = Date.now() + 1500;
    playClearSound();
  } else {
    state.message = "正解。次の一手へ。";
  }
  render();
  return true;
}

function undoOneTurn() {
  if (state.history.length === 0) return;
  const prev = state.history.pop();
  state.gameState = prev.gameState;
  state.ply = prev.ply;
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
  const target = boardPiece(x, y);

  if (state.selectedHand) {
    tryUserMove({ drop: state.selectedHand, to: [x, y], promote: false });
    return;
  }

  if (!state.selectedSquare) {
    if (target && target.owner === "attacker" && target.type !== "K") {
      state.selectedSquare = [x, y];
      render();
    }
    return;
  }

  const [fx, fy] = state.selectedSquare;
  if (fx === x && fy === y) {
    state.selectedSquare = null;
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
    tryUserMove(moveBase);
    return;
  }
  if (moving.type === "P" && y === 1) {
    tryUserMove({ ...moveBase, promote: true });
    return;
  }
  state.promotionPrompt = moveBase;
  render();
}

function choosePromotion(promote) {
  if (!state.promotionPrompt) return;
  const move = { ...state.promotionPrompt, promote };
  state.promotionPrompt = null;
  const ok = tryUserMove(move);
  if (!ok) {
    // Ensure the tap always updates UI even when the selected promote choice is wrong.
    render();
  }
}

async function copyPuzzleLink() {
  if (!state.puzzle || !state.mateLength) return;
  const url = new URL(window.location.href);
  url.searchParams.set("mate", String(state.mateLength));
  url.searchParams.set("id", String(state.puzzle.id));
  const text = url.toString();
  try {
    await navigator.clipboard.writeText(text);
    state.message = "問題リンクをコピーしました。";
  } catch {
    state.message = `問題リンク: ${text}`;
  }
  render();
}

function renderTitle() {
  return h("section", { class: "panel" }, [
    h("div", { class: "row" }, [soundToggleButton()]),
    h("div", { class: "top-hero" }, [
      h("div", {}, [
        h("h1", {}, "たぬき詰将棋"),
        h("p", {}, "タヌキと一緒に、3手詰・5手詰をサクサク挑戦。"),
        h("div", { class: "row" }, [
          h("a", {
            class: "btn",
            href: "https://github.com/tokuhirom/tanuki-tsume-shogi",
            target: "_blank",
            rel: "noopener noreferrer",
          }, "GitHubで見る"),
        ]),
        h("div", { class: "grid4" }, lengths.map((n) =>
          h("button", { class: "btn primary", onclick: () => goList(n) }, `${n}手詰へ`)
        )),
      ]),
      h("img", { src: "./assets/tanuki.svg", alt: "タヌキ" }),
    ]),
  ]);
}

function renderList() {
  const hasPuzzles = state.puzzles.length > 0;
  return h("section", { class: "panel" }, [
    h("div", { class: "row" }, [
      h("button", { class: "btn", onclick: goTitle }, "タイトルへ戻る"),
      soundToggleButton(),
      h("h2", {}, `${state.mateLength}手詰 - 問題一覧`),
    ]),
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
  return h("div", { class: "row" }, Object.entries(hands)
    .filter(([, c]) => c > 0)
    .map(([piece, count]) => h("button", {
      class: `btn hand-btn${state.selectedHand === piece ? " primary" : ""}`,
      onclick: () => {
        state.selectedHand = state.selectedHand === piece ? null : piece;
        state.selectedSquare = null;
        render();
      },
    }, `${piece} x${count}`))
  );
}

function renderBoard() {
  const view = boardViewport();
  const table = h("table", { class: "board" });
  const kanji = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
  const fileLabel = (x) => String(x);

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
      const text = p && !isHiddenAttackerKing(p) ? pieceToText(p).replace(/^v/, "") : "";
      const pieceNode = text
        ? h("span", { class: `piece${p.owner === "defender" ? " defender" : ""}` }, text)
        : "";
      const edgeTop = y === 1 ? " edge-top" : "";
      const edgeBottom = y === 9 ? " edge-bottom" : "";
      const edgeLeft = x === 1 ? " edge-left" : "";
      const edgeRight = x === 9 ? " edge-right" : "";
      tr.append(h("td", {}, h("button", {
        class: `${selected ? "sel" : ""}${edgeTop}${edgeBottom}${edgeLeft}${edgeRight}`.trim(),
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

function renderSolutionPreview() {
  const list = state.puzzle.solution.map((m, i) => `${i + 1}. ${formatMove(m)}`);
  return h("div", { class: "log" }, [
    h("strong", {}, "手順メモ（開発版）"),
    h("div", {}, list.join(" / ")),
  ]);
}

function renderPuzzle() {
  const isClearFx = Date.now() < state.clearFxUntil;
  const fxNodes = isClearFx
    ? h("div", { class: "fx-sparkles" }, Array.from({ length: 12 }).map((_, i) =>
      h("span", { style: `--i:${i}` }, "◆")
    ))
    : null;

  return h("section", { class: "panel" }, [
    h("div", { class: `puzzle-panel${isClearFx ? " victory" : ""}` }, [
      fxNodes,
      h("div", { class: "row" }, [
      h("button", { class: "btn", onclick: () => goList(state.mateLength) }, "問題一覧へ"),
      h("button", { class: "btn", onclick: undoOneTurn }, "一手戻す"),
      h("button", { class: "btn", onclick: copyPuzzleLink }, "リンクをコピー"),
      soundToggleButton(),
      h("h2", {}, `${state.mateLength}手詰 #${state.puzzle.id}`),
      ]),
      isClearFx ? h("div", { class: "clear-badge" }, "CLEAR!") : null,
      h("p", {}, state.message),
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
      renderSolutionPreview(),
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
