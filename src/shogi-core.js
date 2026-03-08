const BOARD_SIZE = 9;

const PIECE_LABEL = {
  K: "玉",
  R: "飛",
  B: "角",
  G: "金",
  S: "銀",
  N: "桂",
  L: "香",
  P: "歩",
  "+R": "龍",
  "+B": "馬",
  "+S": "全",
  "+N": "圭",
  "+L": "杏",
  "+P": "と",
};

const PROMOTABLE = new Set(["R", "B", "S", "N", "L", "P"]);

const HAND_TYPES = ["R", "B", "G", "S", "N", "L", "P"];

const STEP_MOVES = {
  K: [
    [-1, -1], [0, -1], [1, -1],
    [-1, 0],           [1, 0],
    [-1, 1],  [0, 1],  [1, 1],
  ],
  G: [
    [-1, -1], [0, -1], [1, -1],
    [-1, 0],            [1, 0],
              [0, 1],
  ],
  S: [
    [-1, -1], [0, -1], [1, -1],
    [-1, 1],            [1, 1],
  ],
  N: [[-1, -2], [1, -2]],
  P: [[0, -1]],
  "+P": [
    [-1, -1], [0, -1], [1, -1],
    [-1, 0],            [1, 0],
              [0, 1],
  ],
  "+S": [
    [-1, -1], [0, -1], [1, -1],
    [-1, 0],            [1, 0],
              [0, 1],
  ],
  "+N": [
    [-1, -1], [0, -1], [1, -1],
    [-1, 0],            [1, 0],
              [0, 1],
  ],
  "+L": [
    [-1, -1], [0, -1], [1, -1],
    [-1, 0],            [1, 0],
              [0, 1],
  ],
};

const SLIDING_MOVES = {
  R: [[0, -1], [1, 0], [0, 1], [-1, 0]],
  B: [[1, -1], [1, 1], [-1, 1], [-1, -1]],
  L: [[0, -1]],
  "+R": [[0, -1], [1, 0], [0, 1], [-1, 0]],
  "+B": [[1, -1], [1, 1], [-1, 1], [-1, -1]],
};

const EXTRA_STEPS = {
  "+R": [[-1, -1], [1, -1], [1, 1], [-1, 1]],
  "+B": [[0, -1], [1, 0], [0, 1], [-1, 0]],
};

function inside(x, y) {
  return x >= 1 && x <= BOARD_SIZE && y >= 1 && y <= BOARD_SIZE;
}

function key(x, y) {
  return `${x},${y}`;
}

function unpromote(type) {
  return type.startsWith("+") ? type.slice(1) : type;
}

function promote(type) {
  return PROMOTABLE.has(type) ? `+${type}` : type;
}

function cloneHands(hands) {
  return {
    attacker: { ...hands.attacker },
    defender: { ...hands.defender },
  };
}

export function emptyHands() {
  const h = { attacker: {}, defender: {} };
  for (const t of HAND_TYPES) {
    h.attacker[t] = 0;
    h.defender[t] = 0;
  }
  return h;
}

export function createState({ pieces, hands, sideToMove }) {
  const board = new Map();
  for (const p of pieces) {
    if (p.owner === "attacker" && p.type === "K") continue;
    board.set(key(p.x, p.y), { owner: p.owner, type: p.type });
  }
  const h = hands ? cloneHands(hands) : emptyHands();
  // 詰将棋ルール: 守り方の持ち駒 = 全駒数 - 盤上 - 攻め方持ち駒
  const maxCounts = { R: 2, B: 2, G: 4, S: 4, N: 4, L: 4, P: 18 };
  const used = {};
  for (const t of HAND_TYPES) used[t] = h.attacker[t] || 0;
  for (const p of pieces) {
    if (p.type === "K") continue;
    const base = unpromote(p.type);
    if (base in used) used[base]++;
  }
  for (const t of HAND_TYPES) {
    h.defender[t] = Math.max(0, maxCounts[t] - used[t]);
  }
  return {
    board,
    hands: h,
    sideToMove: sideToMove || "attacker",
  };
}

export function cloneState(state) {
  return {
    board: new Map(state.board),
    hands: cloneHands(state.hands),
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
    hands: cloneHands(state.hands),
    sideToMove: state.sideToMove,
  };
}

function promotionZone(owner, y) {
  if (owner === "attacker") return y <= 3;
  return y >= 7;
}

function transformDir(owner, dx, dy) {
  if (owner === "attacker") return [dx, dy];
  return [-dx, -dy];
}

function kingPos(state, owner) {
  for (const [k, p] of state.board.entries()) {
    if (p.owner === owner && p.type === "K") {
      const [x, y] = k.split(",").map(Number);
      return { x, y };
    }
  }
  return null;
}

function squareOccupied(state, x, y) {
  return state.board.get(key(x, y)) || null;
}

function hasPawnOnFile(state, owner, x) {
  for (let y = 1; y <= 9; y += 1) {
    const p = squareOccupied(state, x, y);
    if (p && p.owner === owner && p.type === "P") return true;
  }
  return false;
}

function isMovePromotionLegal(owner, type, fromY, toY, promoteFlag) {
  if (!PROMOTABLE.has(type)) return !promoteFlag;
  const canPromote = promotionZone(owner, fromY) || promotionZone(owner, toY);
  if (!canPromote) return !promoteFlag;
  if (!promoteFlag) {
    // 行き場のない駒の禁止
    if ((type === "P" || type === "L") && ((owner === "attacker" && toY === 1) || (owner === "defender" && toY === 9))) {
      return false;
    }
    if (type === "N" && ((owner === "attacker" && toY <= 2) || (owner === "defender" && toY >= 8))) {
      return false;
    }
  }
  return true;
}

function pseudoMovesFrom(state, x, y, piece) {
  const out = [];
  const { owner, type } = piece;

  const steps = STEP_MOVES[type] || [];
  for (const [dx, dy] of steps) {
    const [tx, ty] = transformDir(owner, dx, dy);
    const nx = x + tx;
    const ny = y + ty;
    if (!inside(nx, ny)) continue;
    const target = squareOccupied(state, nx, ny);
    if (target && target.owner === owner) continue;

    if (type.startsWith("+")) {
      out.push({ from: [x, y], to: [nx, ny], promote: false });
    } else {
      if (PROMOTABLE.has(type) && (promotionZone(owner, y) || promotionZone(owner, ny))) {
        if (isMovePromotionLegal(owner, type, y, ny, false)) {
          out.push({ from: [x, y], to: [nx, ny], promote: false });
        }
        if (isMovePromotionLegal(owner, type, y, ny, true)) {
          out.push({ from: [x, y], to: [nx, ny], promote: true });
        }
      } else {
        out.push({ from: [x, y], to: [nx, ny], promote: false });
      }
    }
  }

  const slides = SLIDING_MOVES[type] || [];
  for (const [dx, dy] of slides) {
    const [tx, ty] = transformDir(owner, dx, dy);
    let nx = x + tx;
    let ny = y + ty;
    while (inside(nx, ny)) {
      const target = squareOccupied(state, nx, ny);
      if (target && target.owner === owner) break;

      if (type.startsWith("+")) {
        out.push({ from: [x, y], to: [nx, ny], promote: false });
      } else {
        if (PROMOTABLE.has(type) && (promotionZone(owner, y) || promotionZone(owner, ny))) {
          if (isMovePromotionLegal(owner, type, y, ny, false)) {
            out.push({ from: [x, y], to: [nx, ny], promote: false });
          }
          if (isMovePromotionLegal(owner, type, y, ny, true)) {
            out.push({ from: [x, y], to: [nx, ny], promote: true });
          }
        } else {
          out.push({ from: [x, y], to: [nx, ny], promote: false });
        }
      }

      if (target && target.owner !== owner) break;
      nx += tx;
      ny += ty;
    }
  }

  const extras = EXTRA_STEPS[type] || [];
  for (const [dx, dy] of extras) {
    const [tx, ty] = transformDir(owner, dx, dy);
    const nx = x + tx;
    const ny = y + ty;
    if (!inside(nx, ny)) continue;
    const target = squareOccupied(state, nx, ny);
    if (target && target.owner === owner) continue;
    out.push({ from: [x, y], to: [nx, ny], promote: false });
  }

  return out;
}

function pseudoDrops(state, owner) {
  const out = [];
  for (const type of HAND_TYPES) {
    const count = state.hands[owner][type] || 0;
    if (count <= 0) continue;
    for (let y = 1; y <= 9; y += 1) {
      for (let x = 1; x <= 9; x += 1) {
        if (squareOccupied(state, x, y)) continue;
        // 行き場のない場所への打ち駒禁止
        if ((type === "P" || type === "L") && ((owner === "attacker" && y === 1) || (owner === "defender" && y === 9))) continue;
        if (type === "N" && ((owner === "attacker" && y <= 2) || (owner === "defender" && y >= 8))) continue;
        if (type === "P" && hasPawnOnFile(state, owner, x)) continue;
        out.push({ drop: type, to: [x, y], promote: false });
      }
    }
  }
  return out;
}

export function applyMove(state, move) {
  const next = cloneState(state);
  const owner = state.sideToMove;

  if (move.drop) {
    const type = move.drop;
    const count = next.hands[owner][type] || 0;
    if (count <= 0) throw new Error("hand piece not found");
    const [x, y] = move.to;
    if (squareOccupied(next, x, y)) throw new Error("drop target occupied");
    next.hands[owner][type] = count - 1;
    next.board.set(key(x, y), { owner, type });
  } else {
    const [fx, fy] = move.from;
    const fromKey = key(fx, fy);
    const src = next.board.get(fromKey);
    if (!src || src.owner !== owner) throw new Error("invalid source piece");
    const [tx, ty] = move.to;
    const targetKey = key(tx, ty);
    const captured = next.board.get(targetKey);
    if (captured && captured.owner === owner) throw new Error("cannot capture own piece");

    next.board.delete(fromKey);
    if (captured) {
      const capturedType = unpromote(captured.type);
      if (capturedType !== "K") {
        next.hands[owner][capturedType] = (next.hands[owner][capturedType] || 0) + 1;
      }
    }
    const movedType = move.promote ? promote(src.type) : src.type;
    next.board.set(targetKey, { owner, type: movedType });
  }

  next.sideToMove = owner === "attacker" ? "defender" : "attacker";
  return next;
}

export function isInCheck(state, owner) {
  const k = kingPos(state, owner);
  if (!k) return owner === "defender";
  const enemy = owner === "attacker" ? "defender" : "attacker";
  for (const [pos, p] of state.board.entries()) {
    if (p.owner !== enemy) continue;
    const [x, y] = pos.split(",").map(Number);
    const moves = pseudoMovesFrom(state, x, y, p);
    if (moves.some((m) => m.to[0] === k.x && m.to[1] === k.y)) return true;
  }
  return false;
}

function pawnDropMateForbidden(state, move) {
  if (!move.drop || move.drop !== "P") return false;
  const owner = state.sideToMove;
  const next = applyMove(state, move);
  const enemy = owner === "attacker" ? "defender" : "attacker";
  if (!isInCheck(next, enemy)) return false;
  const replies = legalMoves(next);
  return replies.length === 0;
}

export function legalMoves(state) {
  const owner = state.sideToMove;
  const out = [];

  for (const [pos, p] of state.board.entries()) {
    if (p.owner !== owner) continue;
    const [x, y] = pos.split(",").map(Number);
    const candidates = pseudoMovesFrom(state, x, y, p);
    for (const m of candidates) {
      const next = applyMove(state, m);
      if (!isInCheck(next, owner)) {
        out.push(m);
      }
    }
  }

  // 王手されている場合、ドロップ候補を最適化
  const inCheck = isInCheck(state, owner);
  if (inCheck) {
    const checkers = findCheckers(state, owner);
    if (checkers.length >= 2) {
      // 両王手: ドロップでは逃げられない（玉移動のみ）
      return out;
    }
    if (checkers.length === 1) {
      const ck = checkers[0];
      if (!isSlidingPiece(ck.piece.type)) {
        // 接触王手: 合駒では防げない
        return out;
      }
      // スライド王手: 遮断マスへのドロップのみ生成
      const kp = kingPos(state, owner);
      const interp = interpositionSquares({ x: ck.x, y: ck.y }, kp);
      for (const target of interp) {
        if (squareOccupied(state, target.x, target.y)) continue;
        for (const type of HAND_TYPES) {
          const count = state.hands[owner][type] || 0;
          if (count <= 0) continue;
          // 行き場のない場所への打ち駒禁止
          if ((type === "P" || type === "L") && ((owner === "attacker" && target.y === 1) || (owner === "defender" && target.y === 9))) continue;
          if (type === "N" && ((owner === "attacker" && target.y <= 2) || (owner === "defender" && target.y >= 8))) continue;
          if (type === "P" && hasPawnOnFile(state, owner, target.x)) continue;
          const m = { drop: type, to: [target.x, target.y], promote: false };
          if (pawnDropMateForbidden(state, m)) continue;
          const next = applyMove(state, m);
          if (!isInCheck(next, owner)) {
            out.push(m);
          }
        }
      }
      return out;
    }
  }

  // 王手されていない場合: 通常のドロップ生成
  const drops = pseudoDrops(state, owner);
  for (const m of drops) {
    if (pawnDropMateForbidden(state, m)) continue;
    const next = applyMove(state, m);
    if (!isInCheck(next, owner)) {
      out.push(m);
    }
  }

  return out;
}

/// スライド駒かどうか判定する
function isSlidingPiece(type) {
  return type === "R" || type === "+R" || type === "B" || type === "+B" || type === "L";
}

/// 王手をかけている駒の位置を探す
function findCheckers(state, kingOwner) {
  const k = kingPos(state, kingOwner);
  if (!k) return [];
  const enemy = kingOwner === "attacker" ? "defender" : "attacker";
  const checkers = [];
  for (const [pos, p] of state.board.entries()) {
    if (p.owner !== enemy) continue;
    const [x, y] = pos.split(",").map(Number);
    const moves = pseudoMovesFrom(state, x, y, p);
    if (moves.some((m) => m.to[0] === k.x && m.to[1] === k.y)) {
      checkers.push({ x, y, piece: p });
    }
  }
  return checkers;
}

/// スライド駒と玉の間の遮断可能マスを列挙する（両端を含まない）
function interpositionSquares(checker, king) {
  const dx = Math.sign(king.x - checker.x);
  const dy = Math.sign(king.y - checker.y);
  const squares = [];
  let x = checker.x + dx;
  let y = checker.y + dy;
  while (x !== king.x || y !== king.y) {
    squares.push({ x, y });
    x += dx;
    y += dy;
  }
  return squares;
}

/// 2点間（直線・斜め）の中間にあるか判定する（両端を含まない）
function isBetween(checker, king, target) {
  const dx = king.x - checker.x;
  const dy = king.y - checker.y;
  const tx = target.x - checker.x;
  const ty = target.y - checker.y;
  if (dx === 0 && dy === 0) return false;

  if (dx === 0) {
    return tx === 0 && Math.sign(ty) === Math.sign(dy) && Math.abs(ty) > 0 && Math.abs(ty) < Math.abs(dy);
  } else if (dy === 0) {
    return ty === 0 && Math.sign(tx) === Math.sign(dx) && Math.abs(tx) > 0 && Math.abs(tx) < Math.abs(dx);
  } else if (Math.abs(dx) === Math.abs(dy)) {
    return Math.abs(tx) === Math.abs(ty)
      && Math.sign(tx) === Math.sign(dx)
      && Math.sign(ty) === Math.sign(dy)
      && Math.abs(tx) > 0
      && Math.abs(tx) < Math.abs(dx);
  }
  return false;
}

function moveToString(move) {
  if (move.drop) {
    return `${move.drop}*${move.to[0]}${move.to[1]}`;
  }
  return `${move.from[0]}${move.from[1]}-${move.to[0]}${move.to[1]}${move.promote ? "+" : ""}`;
}

function forcedMateWithin(state, plies, memo) {
  const serial = JSON.stringify(toSerializable(state));
  const memoKey = `${serial}|${plies}`;
  const cached = memo.get(memoKey);
  if (cached) return cached;

  const side = state.sideToMove;
  const enemy = side === "attacker" ? "defender" : "attacker";

  if (side === "defender") {
    // 守り方: 遅延ドロップ生成 — 盤上の手で逃れが見つかればドロップ生成をスキップ
    const boardMoves = legalMoves(state).filter((m) => !m.drop);

    if (plies <= 0) {
      if (boardMoves.length > 0) {
        const res = { mate: false, unique: false, line: [] };
        memo.set(memoKey, res);
        return res;
      }
      // 盤上の手なし: ドロップも確認
      const dropMoves = legalMoves(state).filter((m) => m.drop);
      if (boardMoves.length === 0 && dropMoves.length === 0) {
        const res = { mate: isInCheck(state, side), unique: true, line: [] };
        memo.set(memoKey, res);
        return res;
      }
      const res = { mate: false, unique: false, line: [] };
      memo.set(memoKey, res);
      return res;
    }

    // 無駄合い判定用
    const checkers = findCheckers(state, "defender");
    const slidingChecker = (checkers.length === 1 && isSlidingPiece(checkers[0].piece.type))
      ? checkers[0] : null;
    const defKingPos = kingPos(state, "defender");

    let allMate = true;
    let allUnique = true;
    let bestMove = null;
    let bestLine = [];

    // Phase 1: 盤上の手を先に処理
    for (const m of boardMoves) {
      const result = forcedMateWithin(applyMove(state, m), plies - 1, memo);
      if (!result.mate) {
        allMate = false;
        break; // 盤上の手で逃れ → ドロップ生成をスキップ
      }
      if (!result.unique) allUnique = false;
      if (result.line.length >= bestLine.length) {
        bestMove = m;
        bestLine = result.line;
      }
    }

    // Phase 2: 盤上の手で全て詰む場合のみ、ドロップを生成して処理
    if (allMate) {
      const dropMoves = legalMoves(state).filter((m) => m.drop);

      if (boardMoves.length === 0 && dropMoves.length === 0) {
        const res = { mate: isInCheck(state, side), unique: true, line: [] };
        memo.set(memoKey, res);
        return res;
      }

      for (const m of dropMoves) {
        // 無駄合い判定
        if (plies >= 2 && slidingChecker && defKingPos) {
          const dropPos = { x: m.to[0], y: m.to[1] };
          const ckPos = { x: slidingChecker.x, y: slidingChecker.y };
          if (isBetween(ckPos, defKingPos, dropPos)) {
            const ckType = slidingChecker.piece.type;
            const canPromote = PROMOTABLE.has(ckType)
              && (promotionZone("attacker", slidingChecker.y) || promotionZone("attacker", dropPos.y));
            const recapture = {
              from: [slidingChecker.x, slidingChecker.y],
              to: [dropPos.x, dropPos.y],
              promote: canPromote,
            };
            const afterDrop = applyMove(state, m);
            const afterRecapture = applyMove(afterDrop, recapture);
            const r = forcedMateWithin(afterRecapture, plies - 2, memo);
            if (r.mate) {
              continue; // 無駄合い
            }
          }
        }

        const result = forcedMateWithin(applyMove(state, m), plies - 1, memo);
        if (!result.mate) {
          allMate = false;
          break;
        }
        if (!result.unique) allUnique = false;
        if (result.line.length >= bestLine.length) {
          bestMove = m;
          bestLine = result.line;
        }
      }
    }

    if (!allMate) {
      const res = { mate: false, unique: false, line: [] };
      memo.set(memoKey, res);
      return res;
    }
    if (bestMove) {
      const res = { mate: true, unique: allUnique, line: [bestMove, ...bestLine] };
      memo.set(memoKey, res);
      return res;
    }
    const res = { mate: true, unique: true, line: [] };
    memo.set(memoKey, res);
    return res;
  }

  if (plies <= 0) {
    const res = { mate: false, unique: false, line: [] };
    memo.set(memoKey, res);
    return res;
  }

  // 攻め方: 王手になる手を抽出してソート
  const moves = legalMoves(state);
  const checks = moves.filter((m) => {
    const n = applyMove(state, m);
    return isInCheck(n, enemy);
  });

  const winning = checks
    .map((m) => ({ move: m, result: forcedMateWithin(applyMove(state, m), plies - 1, memo) }))
    .filter((x) => x.result.mate);

  if (winning.length === 0) {
    const res = { mate: false, unique: false, line: [] };
    memo.set(memoKey, res);
    return res;
  }

  winning.sort((a, b) => moveToString(a.move).localeCompare(moveToString(b.move)));
  const best = winning[0];
  const unique = winning.length === 1 && best.result.unique;
  const res = { mate: true, unique, line: [best.move, ...best.result.line] };
  memo.set(memoKey, res);
  return res;
}

export function findBestDefense(state, remainingPlies) {
  const moves = legalMoves(state);
  if (moves.length === 0) return null;

  const memo = new Map();

  // 無駄合い判定用
  const checkers = findCheckers(state, "defender");
  const slidingChecker = (checkers.length === 1 && isSlidingPiece(checkers[0].piece.type))
    ? checkers[0] : null;
  const defKingPos = kingPos(state, "defender");

  // 最長抵抗: 詰みまでのlineが最も長い応手を選ぶ
  let bestMove = moves[0];
  let bestLen = -1;
  for (const m of moves) {
    // 無駄合い判定
    if (m.drop && remainingPlies >= 2 && slidingChecker && defKingPos) {
      const dropPos = { x: m.to[0], y: m.to[1] };
      const ckPos = { x: slidingChecker.x, y: slidingChecker.y };
      if (isBetween(ckPos, defKingPos, dropPos)) {
        const ckType = slidingChecker.piece.type;
        const canPromote = PROMOTABLE.has(ckType)
          && (promotionZone("attacker", slidingChecker.y) || promotionZone("attacker", dropPos.y));
        const recapture = {
          from: [slidingChecker.x, slidingChecker.y],
          to: [dropPos.x, dropPos.y],
          promote: canPromote,
        };
        const afterDrop = applyMove(state, m);
        const afterRecapture = applyMove(afterDrop, recapture);
        const r = forcedMateWithin(afterRecapture, remainingPlies - 2, memo);
        if (r.mate) continue; // 無駄合い → スキップ
      }
    }

    const next = applyMove(state, m);
    const result = forcedMateWithin(next, remainingPlies - 1, memo);
    if (!result.mate) return m;
    if (result.line.length >= bestLen) {
      bestLen = result.line.length;
      bestMove = m;
    }
  }
  return bestMove;
}

export function validateTsumePuzzle(state, mateLength) {
  if (mateLength % 2 === 0 || mateLength <= 0) {
    return { ok: false, reason: "mate length must be positive odd" };
  }
  if (state.sideToMove !== "attacker") {
    return { ok: false, reason: "attacker must move first" };
  }
  const dKing = kingPos(state, "defender");
  if (!dKing) {
    return { ok: false, reason: "defender king is required" };
  }

  const memo = new Map();
  const within = forcedMateWithin(state, mateLength, memo);
  if (!within.mate) {
    return { ok: false, reason: `no forced mate within ${mateLength} plies` };
  }

  if (mateLength >= 3) {
    const shorter = forcedMateWithin(state, mateLength - 2, memo);
    if (shorter.mate) {
      return { ok: false, reason: `shorter mate exists (<= ${mateLength - 2})` };
    }
  }

  if (!within.unique) {
    return { ok: false, reason: "solution is not unique" };
  }

  return {
    ok: true,
    reason: "ok",
    principalVariation: within.line,
  };
}

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
