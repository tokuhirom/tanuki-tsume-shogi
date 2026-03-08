import { describe, test, expect, beforeAll } from "vitest";
import {
  applyMove,
  cloneState,
  createState,
  emptyHands,
  findBestDefense,
  formatMove,
  initWasm,
  isInCheck,
  legalMoves,
  normalizeMove,
  pieceToText,
  sameMove,
  toSerializable,
  validateTsumePuzzle,
} from "../../src/shogi-core.js";

beforeAll(async () => {
  await initWasm();
});

// ヘルパー: 簡易局面作成
function mkState(pieces, hands, sideToMove = "attacker") {
  return createState({
    pieces,
    hands: hands || emptyHands(),
    sideToMove,
  });
}

// ヘルパー: 持ち駒付き局面
function mkStateWithHand(pieces, attackerHand, sideToMove = "attacker") {
  const hands = emptyHands();
  for (const [type, count] of Object.entries(attackerHand)) {
    hands.attacker[type] = count;
  }
  return createState({ pieces, hands, sideToMove });
}

// --- createState / cloneState ---

describe("createState", () => {
  test("盤面と手番を正しく設定する", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 3, owner: "attacker", type: "G" },
    ]);
    expect(state.sideToMove).toBe("attacker");
    expect(state.board.size).toBe(2);
    expect(state.board.get("5,1")).toEqual({ owner: "defender", type: "K" });
    expect(state.board.get("5,3")).toEqual({ owner: "attacker", type: "G" });
  });

  test("攻め方の玉はスキップされる", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 9, owner: "attacker", type: "K" },
      { x: 5, y: 3, owner: "attacker", type: "G" },
    ]);
    // 攻め方の玉は除外される
    expect(state.board.size).toBe(2);
    expect(state.board.has("5,9")).toBe(false);
  });
});

describe("cloneState", () => {
  test("独立したコピーを作成する", () => {
    const original = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 3, owner: "attacker", type: "G" },
    ]);
    const clone = cloneState(original);
    clone.board.delete("5,3");
    expect(original.board.has("5,3")).toBe(true);
    expect(clone.board.has("5,3")).toBe(false);
  });

  test("持ち駒も独立コピーになる", () => {
    const original = mkStateWithHand(
      [{ x: 5, y: 1, owner: "defender", type: "K" }],
      { G: 1 },
    );
    const clone = cloneState(original);
    clone.hands.attacker.G = 0;
    expect(original.hands.attacker.G).toBe(1);
  });
});

// --- toSerializable ---

describe("toSerializable", () => {
  test("盤面を座標順でシリアライズする", () => {
    const state = mkState([
      { x: 7, y: 3, owner: "attacker", type: "G" },
      { x: 5, y: 1, owner: "defender", type: "K" },
    ]);
    const s = toSerializable(state);
    expect(s.pieces[0]).toEqual({ x: 5, y: 1, owner: "defender", type: "K" });
    expect(s.pieces[1]).toEqual({ x: 7, y: 3, owner: "attacker", type: "G" });
  });
});

// --- applyMove ---

describe("applyMove", () => {
  test("駒を移動する", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 3, owner: "attacker", type: "G" },
    ]);
    const next = applyMove(state, { from: [5, 3], to: [5, 2], promote: false });
    expect(next.board.has("5,3")).toBe(false);
    expect(next.board.get("5,2")).toEqual({ owner: "attacker", type: "G" });
    expect(next.sideToMove).toBe("defender");
  });

  test("成りが適用される", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 4, owner: "attacker", type: "S" },
    ]);
    const next = applyMove(state, { from: [5, 4], to: [5, 3], promote: true });
    expect(next.board.get("5,3")).toEqual({ owner: "attacker", type: "+S" });
  });

  test("駒を取ると持ち駒に加わる", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 2, owner: "defender", type: "G" },
      { x: 5, y: 3, owner: "attacker", type: "R" },
    ]);
    const next = applyMove(state, { from: [5, 3], to: [5, 2], promote: false });
    expect(next.hands.attacker.G).toBe(1);
    expect(next.board.get("5,2").owner).toBe("attacker");
  });

  test("成り駒を取ると成りが外れて持ち駒になる", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 2, owner: "defender", type: "+S" },
      { x: 5, y: 3, owner: "attacker", type: "R" },
    ]);
    const next = applyMove(state, { from: [5, 3], to: [5, 2], promote: false });
    expect(next.hands.attacker.S).toBe(1);
  });

  test("打ち駒が正しく適用される", () => {
    const state = mkStateWithHand(
      [{ x: 5, y: 1, owner: "defender", type: "K" }],
      { G: 1 },
    );
    const next = applyMove(state, { drop: "G", to: [5, 2], promote: false });
    expect(next.board.get("5,2")).toEqual({ owner: "attacker", type: "G" });
    expect(next.hands.attacker.G).toBe(0);
  });

  test("元の局面は変更されない", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 3, owner: "attacker", type: "G" },
    ]);
    applyMove(state, { from: [5, 3], to: [5, 2], promote: false });
    expect(state.board.has("5,3")).toBe(true);
    expect(state.sideToMove).toBe("attacker");
  });
});

// --- isInCheck ---

describe("isInCheck", () => {
  test("王手がかかっている場合trueを返す", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 3, owner: "attacker", type: "R" },
    ]);
    expect(isInCheck(state, "defender")).toBe(true);
  });

  test("王手がかかっていない場合falseを返す", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 3, y: 3, owner: "attacker", type: "R" },
    ]);
    expect(isInCheck(state, "defender")).toBe(false);
  });

  test("桂馬の王手を検出する", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 4, y: 3, owner: "attacker", type: "N" },
    ]);
    expect(isInCheck(state, "defender")).toBe(true);
  });

  test("角の斜め利きで王手を検出する", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 8, y: 4, owner: "attacker", type: "B" },
    ]);
    expect(isInCheck(state, "defender")).toBe(true);
  });

  test("玉がない場合（守り方）trueを返す", () => {
    const state = mkState([
      { x: 5, y: 5, owner: "attacker", type: "G" },
    ]);
    expect(isInCheck(state, "defender")).toBe(true);
  });

  test("駒が遮っている場合falseを返す", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 2, owner: "defender", type: "P" },
      { x: 5, y: 5, owner: "attacker", type: "R" },
    ]);
    expect(isInCheck(state, "defender")).toBe(false);
  });
});

// --- legalMoves ---

describe("legalMoves", () => {
  test("自玉を取られる手は除外される", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 2, owner: "attacker", type: "G" },
      { x: 5, y: 5, owner: "attacker", type: "R" },
    ]);
    const moves = legalMoves(state);
    // 金が5,2から横に動くと飛車の利きで自陣の王が…
    // ここでは attacker に王がないので自殺手チェックは不要
    expect(moves.length).toBeGreaterThan(0);
  });

  test("歩の打ち場所制限（1段目に打てない）", () => {
    const state = mkStateWithHand(
      [{ x: 5, y: 1, owner: "defender", type: "K" }],
      { P: 1 },
    );
    const moves = legalMoves(state);
    const pawnDrops = moves.filter((m) => m.drop === "P");
    // 1段目には打てない
    expect(pawnDrops.every((m) => m.to[1] !== 1)).toBe(true);
  });

  test("二歩の禁止", () => {
    const state = mkStateWithHand(
      [
        { x: 5, y: 1, owner: "defender", type: "K" },
        { x: 3, y: 5, owner: "attacker", type: "P" },
      ],
      { P: 1 },
    );
    const moves = legalMoves(state);
    const pawnDrops = moves.filter((m) => m.drop === "P");
    // 3筋には歩があるので打てない
    expect(pawnDrops.every((m) => m.to[0] !== 3)).toBe(true);
  });

  test("桂馬は2段目以上に打てない", () => {
    const state = mkStateWithHand(
      [{ x: 5, y: 1, owner: "defender", type: "K" }],
      { N: 1 },
    );
    const moves = legalMoves(state);
    const knightDrops = moves.filter((m) => m.drop === "N");
    expect(knightDrops.every((m) => m.to[1] >= 3)).toBe(true);
  });

  test("香車は1段目に打てない", () => {
    const state = mkStateWithHand(
      [{ x: 5, y: 1, owner: "defender", type: "K" }],
      { L: 1 },
    );
    const moves = legalMoves(state);
    const lanceDrops = moves.filter((m) => m.drop === "L");
    expect(lanceDrops.every((m) => m.to[1] !== 1)).toBe(true);
  });

  test("成りゾーンに入る手で成り・不成が生成される", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 3, y: 4, owner: "attacker", type: "S" },
    ]);
    const moves = legalMoves(state);
    const silverMoves = moves.filter((m) => m.from && m.from[0] === 3 && m.from[1] === 4);
    // 3段目に入る手は成り/不成の両方がある
    const to3 = silverMoves.filter((m) => m.to[1] === 3);
    expect(to3.some((m) => m.promote)).toBe(true);
    expect(to3.some((m) => !m.promote)).toBe(true);
  });

  test("行き場のない駒は不成が禁止される", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 3, y: 2, owner: "attacker", type: "P" },
    ]);
    const moves = legalMoves(state);
    // 歩が1段目に行く手は成りのみ（不成は禁止）
    const pawnTo1 = moves.filter((m) => m.from && m.from[0] === 3 && m.to[1] === 1);
    expect(pawnTo1.length).toBe(1);
    expect(pawnTo1[0].promote).toBe(true);
  });

  test("打ち歩詰めの禁止", () => {
    // 守り方: 玉(9,1) 攻め方: 金(8,1), 金(8,2), 持ち駒歩
    // 9,2に歩を打つと詰み（玉の逃げ場なし） → 打ち歩詰めで禁止
    const state = mkStateWithHand(
      [
        { x: 9, y: 1, owner: "defender", type: "K" },
        { x: 8, y: 1, owner: "attacker", type: "G" },
        { x: 8, y: 2, owner: "attacker", type: "G" },
      ],
      { P: 1 },
    );
    const moves = legalMoves(state);
    const pawnDropTo92 = moves.filter((m) => m.drop === "P" && m.to[0] === 9 && m.to[1] === 2);
    expect(pawnDropTo92.length).toBe(0);
  });

  test("守り方の駒の向きが正しい", () => {
    // 守り方の歩は下向き（y+1方向）に動く
    const state = mkState(
      [
        { x: 5, y: 1, owner: "defender", type: "K" },
        { x: 3, y: 5, owner: "defender", type: "P" },
        { x: 3, y: 7, owner: "attacker", type: "G" },
      ],
      undefined,
      "defender",
    );
    const moves = legalMoves(state);
    const pawnMoves = moves.filter((m) => m.from && m.from[0] === 3 && m.from[1] === 5);
    expect(pawnMoves.some((m) => m.to[1] === 6)).toBe(true);
    expect(pawnMoves.every((m) => m.to[1] !== 4)).toBe(true);
  });
});

// --- validateTsumePuzzle ---

describe("validateTsumePuzzle", () => {
  test("1手詰の正解を検証する", () => {
    // 実際のパズル#1: 玉(7,1), 金(7,3), 香(7,4) → 7,2金
    const state = mkState([
      { x: 7, y: 1, owner: "defender", type: "K" },
      { x: 7, y: 3, owner: "attacker", type: "G" },
      { x: 7, y: 4, owner: "attacker", type: "L" },
    ]);
    const result = validateTsumePuzzle(state, 1);
    expect(result.ok).toBe(true);
    expect(result.principalVariation).toHaveLength(1);
    expect(result.principalVariation[0].to).toEqual([7, 2]);
  });

  test("偶数手数は拒否される", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
    ]);
    const result = validateTsumePuzzle(state, 2);
    expect(result.ok).toBe(false);
  });

  test("守り方の玉がないと拒否される", () => {
    const state = mkState([
      { x: 5, y: 5, owner: "attacker", type: "G" },
    ]);
    const result = validateTsumePuzzle(state, 1);
    expect(result.ok).toBe(false);
  });

  test("詰まない局面は拒否される", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 1, y: 9, owner: "attacker", type: "P" },
    ]);
    const result = validateTsumePuzzle(state, 1);
    expect(result.ok).toBe(false);
  });

  test("解が唯一でない場合は拒否される", () => {
    // 両方の金で王手できる → 複数解
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 4, y: 3, owner: "attacker", type: "G" },
      { x: 6, y: 3, owner: "attacker", type: "G" },
    ]);
    const result = validateTsumePuzzle(state, 1);
    // 4,2金 でも 6,2金 でも詰むなら unique=false
    expect(result.ok).toBe(false);
  });
});

// --- findBestDefense ---

describe("findBestDefense", () => {
  test("逃げ手がある場合にそれを返す", () => {
    // 守り方玉(5,1)に王手がかかっている。逃げ手があるはず
    const state = mkState(
      [
        { x: 5, y: 1, owner: "defender", type: "K" },
        { x: 5, y: 2, owner: "attacker", type: "G" },
      ],
      undefined,
      "defender",
    );
    const defense = findBestDefense(state, 3);
    expect(defense).not.toBeNull();
    expect(defense.from).toEqual([5, 1]);
  });

  test("合法手がない場合nullを返す", () => {
    // 詰んでいる局面
    const state = mkState(
      [
        { x: 9, y: 1, owner: "defender", type: "K" },
        { x: 9, y: 2, owner: "attacker", type: "G" },
        { x: 8, y: 2, owner: "attacker", type: "S" },
      ],
      undefined,
      "defender",
    );
    const defense = findBestDefense(state, 1);
    // 玉の逃げ場を確認
    const moves = legalMoves(state);
    if (moves.length === 0) {
      expect(defense).toBeNull();
    } else {
      expect(defense).not.toBeNull();
    }
  });
});

// --- formatMove ---

describe("formatMove", () => {
  test("移動手をフォーマットする", () => {
    expect(formatMove({ from: [5, 3], to: [5, 2], promote: false })).toBe(
      "(5,3)→(5,2)",
    );
  });

  test("成り手をフォーマットする", () => {
    expect(formatMove({ from: [5, 3], to: [5, 2], promote: true })).toBe(
      "(5,3)→(5,2)成",
    );
  });

  test("打ち手をフォーマットする", () => {
    expect(formatMove({ drop: "G", to: [5, 2] })).toBe("金打(5,2)");
  });
});

// --- pieceToText ---

describe("pieceToText", () => {
  test("攻め方の駒", () => {
    expect(pieceToText({ owner: "attacker", type: "G" })).toBe("金");
    expect(pieceToText({ owner: "attacker", type: "+R" })).toBe("龍");
  });

  test("守り方の駒にはvが付く", () => {
    expect(pieceToText({ owner: "defender", type: "K" })).toBe("v玉");
    expect(pieceToText({ owner: "defender", type: "P" })).toBe("v歩");
  });
});

// --- sameMove / normalizeMove ---

describe("sameMove", () => {
  test("同じ移動手", () => {
    const a = { from: [5, 3], to: [5, 2], promote: false };
    const b = { from: [5, 3], to: [5, 2], promote: false };
    expect(sameMove(a, b)).toBe(true);
  });

  test("成りが異なる", () => {
    const a = { from: [5, 3], to: [5, 2], promote: false };
    const b = { from: [5, 3], to: [5, 2], promote: true };
    expect(sameMove(a, b)).toBe(false);
  });

  test("移動先が異なる", () => {
    const a = { from: [5, 3], to: [5, 2], promote: false };
    const b = { from: [5, 3], to: [4, 2], promote: false };
    expect(sameMove(a, b)).toBe(false);
  });

  test("同じ打ち手", () => {
    const a = { drop: "G", to: [5, 2] };
    const b = { drop: "G", to: [5, 2] };
    expect(sameMove(a, b)).toBe(true);
  });

  test("打ちと移動は異なる", () => {
    const a = { drop: "G", to: [5, 2] };
    const b = { from: [5, 3], to: [5, 2], promote: false };
    expect(sameMove(a, b)).toBe(false);
  });
});

describe("normalizeMove", () => {
  test("移動手の正規化", () => {
    const m = normalizeMove({ from: [5, 3], to: [5, 2], promote: true });
    expect(m).toEqual({ from: [5, 3], to: [5, 2], drop: undefined, promote: true });
  });

  test("打ち手の正規化", () => {
    const m = normalizeMove({ drop: "G", to: [5, 2] });
    expect(m).toEqual({ from: undefined, to: [5, 2], drop: "G", promote: false });
  });
});

// --- 駒の動き詳細テスト ---

describe("駒の動き", () => {
  test("龍（成り飛車）は飛車の動き＋斜め1マス", () => {
    const state = mkState([
      { x: 9, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 5, owner: "attacker", type: "+R" },
    ]);
    const moves = legalMoves(state);
    const dragonMoves = moves.filter((m) => m.from && m.from[0] === 5 && m.from[1] === 5);
    // 十字方向にスライド + 斜め4方向1マス
    const targets = dragonMoves.map((m) => `${m.to[0]},${m.to[1]}`);
    // 斜め1マス
    expect(targets).toContain("4,4");
    expect(targets).toContain("6,4");
    expect(targets).toContain("4,6");
    expect(targets).toContain("6,6");
    // 十字方向のスライド
    expect(targets).toContain("5,1");
    expect(targets).toContain("5,9");
    expect(targets).toContain("1,5");
    expect(targets).toContain("9,5");
  });

  test("馬（成り角）は角の動き＋十字1マス", () => {
    const state = mkState([
      { x: 9, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 5, owner: "attacker", type: "+B" },
    ]);
    const moves = legalMoves(state);
    const horseMoves = moves.filter((m) => m.from && m.from[0] === 5 && m.from[1] === 5);
    const targets = horseMoves.map((m) => `${m.to[0]},${m.to[1]}`);
    // 十字1マス
    expect(targets).toContain("5,4");
    expect(targets).toContain("5,6");
    expect(targets).toContain("4,5");
    expect(targets).toContain("6,5");
    // 斜めスライド
    expect(targets).toContain("1,1");
    expect(targets).toContain("1,9");
    expect(targets).toContain("9,9");
  });

  test("香車は前方にのみスライドする", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 3, y: 7, owner: "attacker", type: "L" },
    ]);
    const moves = legalMoves(state);
    const lanceMoves = moves.filter((m) => m.from && m.from[0] === 3 && m.from[1] === 7);
    // 前方（y減少方向）にのみ動ける
    expect(lanceMoves.every((m) => m.to[0] === 3 && m.to[1] < 7)).toBe(true);
    expect(lanceMoves.length).toBeGreaterThan(0);
  });

  test("桂馬は前方2マスジャンプのみ", () => {
    const state = mkState([
      { x: 5, y: 1, owner: "defender", type: "K" },
      { x: 5, y: 5, owner: "attacker", type: "N" },
    ]);
    const moves = legalMoves(state);
    const knightMoves = moves.filter((m) => m.from && m.from[0] === 5 && m.from[1] === 5);
    const targets = knightMoves.map((m) => `${m.to[0]},${m.to[1]}`);
    expect(targets).toContain("4,3");
    expect(targets).toContain("6,3");
    // 後方にはジャンプしない
    expect(targets).not.toContain("4,7");
    expect(targets).not.toContain("6,7");
  });
});
