//! 詰将棋ソルバー — ブラウザ向け（findBestDefense 等）
//!
//! JS の forcedMateWithin / findBestDefense を Rust に移植したもの。
//! WASM 経由でブラウザから利用する。

use crate::shogi::*;
use rustc_hash::FxHashMap;

/// 探索結果
#[derive(Clone, Debug)]
struct MateResult {
    mate: bool,
    unique: bool,
    line: Vec<Move>,
}

/// メモ化キー: Zobrist ハッシュ + 残り手数
type MemoKey = (u64, u32);

/// 指定手数以内で詰むか判定する（JS の forcedMateWithin 相当）
fn forced_mate_within(state: &mut State, plies: u32, memo: &mut FxHashMap<MemoKey, MateResult>) -> MateResult {
    let key = (state.zobrist_hash, plies);
    if let Some(cached) = memo.get(&key) {
        return cached.clone();
    }

    let side = state.side_to_move;

    if side == Owner::Defender {
        return forced_mate_within_defender(state, plies, memo);
    }

    // 攻め方（OR ノード）
    if plies == 0 {
        let res = MateResult { mate: false, unique: false, line: vec![] };
        memo.insert(key, res.clone());
        return res;
    }

    // 王手になる手だけを直接生成
    let checks = legal_check_moves(state);

    let mut winning: Vec<(Move, MateResult)> = Vec::new();
    for m in &checks {
        let undo = make_move(state, m);
        let result = forced_mate_within(state, plies - 1, memo);
        undo_move(state, m, &undo);
        if result.mate {
            winning.push((m.clone(), result));
        }
    }

    if winning.is_empty() {
        let res = MateResult { mate: false, unique: false, line: vec![] };
        memo.insert(key, res.clone());
        return res;
    }

    // 安定したソート（手の文字列表現）
    winning.sort_by(|a, b| move_to_string(&a.0).cmp(&move_to_string(&b.0)));
    let best = &winning[0];
    let unique = winning.len() == 1 && best.1.unique;
    let mut line = vec![best.0.clone()];
    line.extend(best.1.line.iter().cloned());
    let res = MateResult { mate: true, unique, line };
    memo.insert(key, res.clone());
    res
}

/// 守り方のターン処理
fn forced_mate_within_defender(state: &mut State, plies: u32, memo: &mut FxHashMap<MemoKey, MateResult>) -> MateResult {
    let key = (state.zobrist_hash, plies);

    // 盤上の手を先に生成
    let board_moves = legal_board_moves(state);

    if plies == 0 {
        if !board_moves.is_empty() {
            let res = MateResult { mate: false, unique: false, line: vec![] };
            memo.insert(key, res.clone());
            return res;
        }
        let drop_moves = legal_drop_moves(state);
        if board_moves.is_empty() && drop_moves.is_empty() {
            let res = MateResult {
                mate: is_in_check(state, Owner::Defender),
                unique: true,
                line: vec![],
            };
            memo.insert(key, res.clone());
            return res;
        }
        let res = MateResult { mate: false, unique: false, line: vec![] };
        memo.insert(key, res.clone());
        return res;
    }

    // 無駄合い判定用
    let checkers = find_checkers(state, Owner::Defender);
    let sliding_checker = if checkers.len() == 1 && is_sliding_piece(checkers[0].1.piece_type) {
        Some(checkers[0])
    } else {
        None
    };
    let def_king_pos = state.king_pos(Owner::Defender);

    let mut all_mate = true;
    let mut all_unique = true;
    let mut best_move: Option<Move> = None;
    let mut best_line: Vec<Move> = vec![];

    // 無駄合い判定の共通処理（ドロップ・移動合い両対応）
    let is_wasteful = |state: &mut State, m: &Move, memo: &mut FxHashMap<MemoKey, MateResult>| -> bool {
        if plies < 2 { return false; }
        let (ck_pos, ck_bp) = match &sliding_checker {
            Some(v) => *v,
            None => return false,
        };
        let kp = match def_king_pos {
            Some(p) => p,
            None => return false,
        };
        let to_pos = Pos::new(m.to[0], m.to[1]);
        if !is_between(ck_pos, kp, to_pos) { return false; }
        // 玉の移動は合駒ではない
        if let Some(from) = m.from {
            if let Some(bp) = state.get(Pos::new(from[0], from[1])) {
                if bp.piece_type == PieceType::K { return false; }
            }
        }
        let can_promote = ck_bp.piece_type.is_promotable()
            && (promotion_zone(Owner::Attacker, ck_pos.y)
                || promotion_zone(Owner::Attacker, to_pos.y));
        let recapture = Move {
            from: Some([ck_pos.x, ck_pos.y]),
            to: [to_pos.x, to_pos.y],
            promote: can_promote,
            drop: None,
        };
        let undo1 = make_move(state, m);
        let undo2 = make_move(state, &recapture);
        let r = forced_mate_within(state, plies - 2, memo);
        undo_move(state, &recapture, &undo2);
        undo_move(state, m, &undo1);
        r.mate
    };

    // Phase 1: 盤上の手（移動合いの無駄合い判定付き）
    for m in &board_moves {
        if is_wasteful(state, m, memo) {
            continue;
        }
        let undo = make_move(state, m);
        let result = forced_mate_within(state, plies - 1, memo);
        undo_move(state, m, &undo);
        if !result.mate {
            all_mate = false;
            break;
        }
        if !result.unique { all_unique = false; }
        if result.line.len() >= best_line.len() {
            best_move = Some(m.clone());
            best_line = result.line.clone();
        }
    }

    // Phase 2: ドロップ（盤上の手で全て詰む場合のみ）
    if all_mate {
        let drop_moves = legal_drop_moves(state);

        if board_moves.is_empty() && drop_moves.is_empty() {
            let res = MateResult {
                mate: is_in_check(state, Owner::Defender),
                unique: true,
                line: vec![],
            };
            memo.insert(key, res.clone());
            return res;
        }

        for m in &drop_moves {
            if is_wasteful(state, m, memo) {
                continue;
            }

            let undo = make_move(state, m);
            let result = forced_mate_within(state, plies - 1, memo);
            undo_move(state, m, &undo);
            if !result.mate {
                all_mate = false;
                break;
            }
            if !result.unique { all_unique = false; }
            if result.line.len() >= best_line.len() {
                best_move = Some(m.clone());
                best_line = result.line.clone();
            }
        }
    }

    if !all_mate {
        let res = MateResult { mate: false, unique: false, line: vec![] };
        memo.insert(key, res.clone());
        return res;
    }
    if let Some(bm) = best_move {
        let mut line = vec![bm];
        line.extend(best_line);
        let res = MateResult { mate: true, unique: all_unique, line };
        memo.insert(key, res.clone());
        return res;
    }
    let res = MateResult { mate: true, unique: true, line: vec![] };
    memo.insert(key, res.clone());
    res
}

/// 合法手を全て生成する（盤上の手 + ドロップ）
fn generate_legal_moves(state: &mut State) -> Vec<Move> {
    let mut moves = legal_board_moves(state);
    moves.extend(legal_drop_moves(state));
    moves
}

/// 手の文字列表現（ソート用）
fn move_to_string(m: &Move) -> String {
    if let Some(drop_type) = m.drop {
        format!("{:?}*{}{}", drop_type, m.to[0], m.to[1])
    } else {
        let from = m.from.unwrap();
        format!("{}{}-{}{}{}", from[0], from[1], m.to[0], m.to[1], if m.promote { "+" } else { "" })
    }
}

/// 守り方の最善応手を探す（最長抵抗）
pub fn find_best_defense(state: &mut State, remaining_plies: u32) -> Option<Move> {
    let moves = generate_legal_moves(state);
    if moves.is_empty() { return None; }

    let mut memo: FxHashMap<MemoKey, MateResult> = FxHashMap::default();

    // 無駄合い判定用
    let checkers = find_checkers(state, Owner::Defender);
    let sliding_checker = if checkers.len() == 1 && is_sliding_piece(checkers[0].1.piece_type) {
        Some(checkers[0])
    } else {
        None
    };
    let def_king_pos = state.king_pos(Owner::Defender);

    let mut best_move = moves[0].clone();
    let mut best_len: i32 = -1;

    for m in &moves {
        // 無駄合い判定（ドロップ・移動合い両対応）
        if remaining_plies >= 2 {
            if let (Some((ck_pos, ck_bp)), Some(kp)) = (&sliding_checker, def_king_pos) {
                let to_pos = Pos::new(m.to[0], m.to[1]);
                if is_between(*ck_pos, kp, to_pos) {
                    // 玉の移動は合駒ではない
                    let is_king_move = m.from.is_some_and(|from| {
                        state.get(Pos::new(from[0], from[1]))
                            .is_some_and(|bp| bp.piece_type == PieceType::K)
                    });
                    if !is_king_move {
                        let can_promote = ck_bp.piece_type.is_promotable()
                            && (promotion_zone(Owner::Attacker, ck_pos.y)
                                || promotion_zone(Owner::Attacker, to_pos.y));
                        let recapture = Move {
                            from: Some([ck_pos.x, ck_pos.y]),
                            to: [to_pos.x, to_pos.y],
                            promote: can_promote,
                            drop: None,
                        };
                        let undo1 = make_move(state, m);
                        let undo2 = make_move(state, &recapture);
                        let r = forced_mate_within(state, remaining_plies - 2, &mut memo);
                        undo_move(state, &recapture, &undo2);
                        undo_move(state, m, &undo1);
                        if r.mate {
                            continue; // 無駄合い → スキップ
                        }
                    }
                }
            }
        }

        let undo = make_move(state, m);
        let result = forced_mate_within(state, remaining_plies - 1, &mut memo);
        undo_move(state, m, &undo);

        if !result.mate {
            return Some(m.clone()); // 逃れ手
        }
        if result.line.len() as i32 >= best_len {
            best_len = result.line.len() as i32;
            best_move = m.clone();
        }
    }

    Some(best_move)
}

/// 詰将棋パズルを検証する（JS の validateTsumePuzzle 相当）
pub fn validate_tsume_puzzle_js(state: &mut State, mate_length: u32) -> Option<Vec<Move>> {
    if mate_length.is_multiple_of(2) || mate_length == 0 {
        return None;
    }
    if state.side_to_move != Owner::Attacker {
        return None;
    }
    state.king_pos(Owner::Defender)?;
    if state_has_dead_end_pieces(state) {
        return None;
    }

    let mut memo: FxHashMap<MemoKey, MateResult> = FxHashMap::default();

    // 短い手数で詰まないことを確認
    let mut d = 1;
    while d < mate_length {
        let r = forced_mate_within(state, d, &mut memo);
        if r.mate {
            return None; // より短い手数で詰む
        }
        d += 2;
    }

    let result = forced_mate_within(state, mate_length, &mut memo);
    if !result.mate || !result.unique {
        return None;
    }

    Some(result.line)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 唯一解の1手詰局面: △K(1,1), ▲G(1,2), ▲G(2,2)
    /// 唯一の詰み手は ▲G(1,2)→(1,1) ... ではなく王を取ってしまう
    /// → 実際のパズルファイルを使う
    #[test]
    fn test_validate_puzzle_file_1mate() {
        let file = "../generator/puzzles/1.json";
        if !std::path::Path::new(file).exists() { return; }
        let data = std::fs::read_to_string(file).unwrap();
        let puzzles: Vec<serde_json::Value> = serde_json::from_str(&data).unwrap();
        if puzzles.is_empty() { return; }

        // 最初のパズルで検証
        let p = &puzzles[0];
        let initial: InitialData = serde_json::from_value(p["initial"].clone()).unwrap();
        let mut state = initial.to_state();
        let result = validate_tsume_puzzle_js(&mut state, 1);
        assert!(result.is_some(), "1手詰パズルの検証に失敗");
        assert_eq!(result.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_validate_puzzle_file_3mate() {
        let file = "../generator/puzzles/3.json";
        if !std::path::Path::new(file).exists() { return; }
        let data = std::fs::read_to_string(file).unwrap();
        let puzzles: Vec<serde_json::Value> = serde_json::from_str(&data).unwrap();

        for (i, p) in puzzles.iter().take(5).enumerate() {
            let initial: InitialData = serde_json::from_value(p["initial"].clone()).unwrap();
            let mut state = initial.to_state();
            let result = validate_tsume_puzzle_js(&mut state, 3);
            assert!(result.is_some(), "3手詰パズル {} の検証に失敗", i);
            assert_eq!(result.as_ref().unwrap().len(), 3);
        }
    }

    #[test]
    fn test_find_best_defense_returns_some() {
        // 3手詰の局面を用意し、1手目後の守り方応手をテスト
        let init = InitialData {
            pieces: vec![
                PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::P },
                PieceData { x: 7, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
                PieceData { x: 9, y: 1, owner: Owner::Defender, piece_type: PieceType::G },
                PieceData { x: 9, y: 3, owner: Owner::Attacker, piece_type: PieceType::R },
                PieceData { x: 6, y: 4, owner: Owner::Attacker, piece_type: PieceType::N },
            ],
            hands: HandsData {
                attacker: HandCount::default(),
                defender: HandCount::default(),
            },
            side_to_move: Owner::Attacker,
        };
        let mut state = init.to_state();

        // 1手目: R(9,3)→(7,3)+
        let m1 = Move { from: Some([9, 3]), to: [7, 3], promote: true, drop: None };
        let undo = make_move(&mut state, &m1);
        assert!(is_in_check(&state, Owner::Defender));

        // 守り方の最善応手
        let defense = find_best_defense(&mut state, 2);
        assert!(defense.is_some());
        let def = defense.unwrap();
        // 無駄合い（P*72等）ではなく、玉移動のはず
        assert!(def.drop.is_none(), "守り方は玉移動のはず（無駄合いスキップ）: {:?}", def);

        undo_move(&mut state, &m1, &undo);
    }

    #[test]
    fn test_validate_tsume_puzzle_js_rejects_dead_end_pieces() {
        let init = InitialData {
            pieces: vec![
                PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
                PieceData { x: 1, y: 2, owner: Owner::Attacker, piece_type: PieceType::N },
            ],
            hands: HandsData {
                attacker: HandCount::default(),
                defender: HandCount::default(),
            },
            side_to_move: Owner::Attacker,
        };
        let mut state = init.to_state();
        assert!(validate_tsume_puzzle_js(&mut state, 1).is_none());
    }
}
