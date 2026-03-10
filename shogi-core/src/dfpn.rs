//! df-pn (Depth-First Proof-Number Search) による詰将棋ソルバー
//!
//! 従来の全幅探索（forced_mate_within）より効率的に詰み/不詰を判定する。
//! 特に長手数（9手以上）で大きな高速化が期待できる。
//!
//! アルゴリズム概要:
//! - 各ノードに proof number (pn) と disproof number (dn) を持たせる
//! - OR ノード（攻め方）: pn = min(子の pn), dn = Σ(子の dn)
//! - AND ノード（守り方）: pn = Σ(子の pn), dn = min(子の dn)
//! - 閾値を超えたら上位に戻り、最善のノードを再展開する（MID アルゴリズム）

use crate::shogi::*;
use rustc_hash::FxHashMap;

const INF: u32 = u32::MAX / 2;

/// デフォルトのノード上限
const DEFAULT_NODE_LIMIT: u64 = 50_000_000;

/// トランスポジションテーブルのエントリ
#[derive(Clone)]
struct TtEntry {
    pn: u32,
    dn: u32,
}

/// df-pn ソルバー
pub struct DfpnSolver {
    tt: FxHashMap<u64, TtEntry>,
    node_count: u64,
    node_limit: u64,
}

/// TT キー: Zobrist ハッシュに残り手数を混ぜる
fn tt_key(state: &State, depth: u32) -> u64 {
    state.zobrist_hash.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(depth as u64)
}

impl Default for DfpnSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DfpnSolver {
    pub fn new() -> Self {
        DfpnSolver {
            tt: FxHashMap::default(),
            node_count: 0,
            node_limit: DEFAULT_NODE_LIMIT,
        }
    }

    #[allow(dead_code)]
    pub fn with_node_limit(mut self, limit: u64) -> Self {
        self.node_limit = limit;
        self
    }

    #[allow(dead_code)]
    pub fn node_count(&self) -> u64 {
        self.node_count
    }

    fn lookup(&self, key: u64) -> (u32, u32) {
        match self.tt.get(&key) {
            Some(e) => (e.pn, e.dn),
            None => (1, 1), // 未展開ノードの初期値
        }
    }

    fn store(&mut self, key: u64, pn: u32, dn: u32) {
        self.tt.insert(key, TtEntry { pn, dn });
    }

    /// 指定局面が depth 手以内で詰むかを判定する
    pub fn solve(&mut self, state: &mut State, depth: u32) -> bool {
        self.node_count = 0;
        self.mid(state, depth, INF, INF);
        let key = tt_key(state, depth);
        let (pn, _) = self.lookup(key);
        pn == 0
    }

    /// MID (Multiple Iterative Deepening) — df-pn のコアループ
    fn mid(&mut self, state: &mut State, depth: u32, th_pn: u32, th_dn: u32) {
        let key = tt_key(state, depth);
        let (pn, dn) = self.lookup(key);
        if pn >= th_pn || dn >= th_dn {
            return;
        }
        if self.node_count >= self.node_limit {
            return;
        }
        self.node_count += 1;

        if state.side_to_move == Owner::Attacker {
            self.mid_or(state, depth, th_pn, th_dn, key);
        } else {
            self.mid_and(state, depth, th_pn, th_dn, key);
        }
    }

    /// OR ノード（攻め方の手番）: 王手になる手のみ探索
    fn mid_or(&mut self, state: &mut State, depth: u32, th_pn: u32, th_dn: u32, key: u64) {
        if depth == 0 {
            self.store(key, INF, 0);
            return;
        }

        let checks = generate_checks(state);
        if checks.is_empty() {
            self.store(key, INF, 0);
            return;
        }

        let n = checks.len();
        let mut child_pn = vec![0u32; n];
        let mut child_dn = vec![0u32; n];

        loop {
            if self.node_count >= self.node_limit {
                let (cur_min_pn, cur_sum_dn) = Self::or_aggregate(&child_pn, &child_dn);
                self.store(key, cur_min_pn, cur_sum_dn);
                return;
            }

            for (i, m) in checks.iter().enumerate() {
                let undo = make_move(state, m);
                let ck = tt_key(state, depth - 1);
                let (p, d) = self.lookup(ck);
                undo_move(state, m, &undo);
                child_pn[i] = p;
                child_dn[i] = d;
            }

            let (min_pn, sum_dn) = Self::or_aggregate(&child_pn, &child_dn);
            self.store(key, min_pn, sum_dn);

            if min_pn >= th_pn || sum_dn >= th_dn {
                return;
            }

            let (best_idx, second_pn) = Self::find_best_or(&child_pn);
            let c_th_pn = th_pn.min(second_pn.saturating_add(1));
            let c_th_dn = th_dn.saturating_sub(sum_dn).saturating_add(child_dn[best_idx]).min(INF);

            let m = checks[best_idx].clone();
            let undo = make_move(state, &m);
            self.mid(state, depth - 1, c_th_pn, c_th_dn);
            undo_move(state, &m, &undo);
        }
    }

    /// AND ノード（守り方の手番）: 無駄合い判定付き
    fn mid_and(&mut self, state: &mut State, depth: u32, th_pn: u32, th_dn: u32, key: u64) {
        let board_moves = legal_board_moves(state);
        let drop_moves = legal_drop_moves(state);

        if board_moves.is_empty() && drop_moves.is_empty() {
            if is_in_check(state, Owner::Defender) {
                self.store(key, 0, INF); // 詰み
            } else {
                self.store(key, INF, 0);
            }
            return;
        }

        if depth == 0 {
            self.store(key, INF, 0);
            return;
        }

        // 無駄合いフィルタ（ドロップ・移動合い両方に適用）
        let all_moves: Vec<Move> = board_moves.into_iter().chain(drop_moves).collect();
        let moves = filter_moves_if_wasteful(state, &all_moves, depth, self);

        let n = moves.len();
        let mut child_pn = vec![0u32; n];
        let mut child_dn = vec![0u32; n];

        loop {
            if self.node_count >= self.node_limit {
                let (cur_sum_pn, cur_min_dn) = Self::and_aggregate(&child_pn, &child_dn);
                self.store(key, cur_sum_pn, cur_min_dn);
                return;
            }

            for (i, m) in moves.iter().enumerate() {
                let undo = make_move(state, m);
                let ck = tt_key(state, depth - 1);
                let (p, d) = self.lookup(ck);
                undo_move(state, m, &undo);
                child_pn[i] = p;
                child_dn[i] = d;
            }

            let (sum_pn, min_dn) = Self::and_aggregate(&child_pn, &child_dn);
            self.store(key, sum_pn, min_dn);

            if sum_pn >= th_pn || min_dn >= th_dn {
                return;
            }

            let (best_idx, second_dn) = Self::find_best_and(&child_dn);
            let c_th_dn = th_dn.min(second_dn.saturating_add(1));
            let c_th_pn = th_pn.saturating_sub(sum_pn).saturating_add(child_pn[best_idx]).min(INF);

            let m = moves[best_idx].clone();
            let undo = make_move(state, &m);
            self.mid(state, depth - 1, c_th_pn, c_th_dn);
            undo_move(state, &m, &undo);
        }
    }

    /// OR ノードの集計: pn = min(子の pn), dn = Σ(子の dn)
    fn or_aggregate(child_pn: &[u32], child_dn: &[u32]) -> (u32, u32) {
        let min_pn = child_pn.iter().copied().min().unwrap_or(INF);
        let sum_dn = child_dn.iter().copied().fold(0u32, |acc, d| acc.saturating_add(d));
        (min_pn, sum_dn.min(INF))
    }

    /// AND ノードの集計: pn = Σ(子の pn), dn = min(子の dn)
    fn and_aggregate(child_pn: &[u32], child_dn: &[u32]) -> (u32, u32) {
        let sum_pn = child_pn.iter().copied().fold(0u32, |acc, p| acc.saturating_add(p));
        let min_dn = child_dn.iter().copied().min().unwrap_or(INF);
        (sum_pn.min(INF), min_dn)
    }

    /// OR ノードで最小 pn のインデックスと二番目の pn を返す
    fn find_best_or(child_pn: &[u32]) -> (usize, u32) {
        let mut best_idx = 0;
        let mut min_pn = INF;
        let mut second_pn = INF;
        for (i, &p) in child_pn.iter().enumerate() {
            if p < min_pn {
                second_pn = min_pn;
                min_pn = p;
                best_idx = i;
            } else if p < second_pn {
                second_pn = p;
            }
        }
        (best_idx, second_pn)
    }

    /// AND ノードで最小 dn のインデックスと二番目の dn を返す
    fn find_best_and(child_dn: &[u32]) -> (usize, u32) {
        let mut best_idx = 0;
        let mut min_dn = INF;
        let mut second_dn = INF;
        for (i, &d) in child_dn.iter().enumerate() {
            if d < min_dn {
                second_dn = min_dn;
                min_dn = d;
                best_idx = i;
            } else if d < second_dn {
                second_dn = d;
            }
        }
        (best_idx, second_dn)
    }

    /// 解の手順を抽出し、唯一解かどうかも検証する
    /// 攻め方の各手番で詰む手が複数あれば None（唯一解でない）
    pub fn extract_unique_solution(&mut self, state: &mut State, depth: u32) -> Option<Vec<Move>> {
        if state.side_to_move == Owner::Attacker {
            if depth == 0 {
                return None;
            }
            let checks = generate_checks(state);

            // 詰む手を列挙（未展開ノードは追加探索で判定）
            let mut proven_moves: Vec<Move> = Vec::new();
            for m in &checks {
                let undo = make_move(state, m);
                let ck = tt_key(state, depth - 1);
                let (cpn, _) = self.lookup(ck);
                undo_move(state, m, &undo);

                if cpn == 0 {
                    proven_moves.push(m.clone());
                } else if cpn < INF {
                    // 未完全展開 — 追加探索で詰むか確認
                    let undo = make_move(state, m);
                    self.mid(state, depth - 1, INF, INF);
                    let ck2 = tt_key(state, depth - 1);
                    let (cpn2, _) = self.lookup(ck2);
                    undo_move(state, m, &undo);
                    if cpn2 == 0 {
                        proven_moves.push(m.clone());
                    }
                }

                if proven_moves.len() > 1 {
                    return None; // 複数の詰み手 → 唯一解でない
                }
            }

            if proven_moves.len() != 1 {
                return None;
            }

            let m = proven_moves.into_iter().next().unwrap();
            let undo = make_move(state, &m);
            let rest = self.extract_unique_solution(state, depth - 1)?;
            undo_move(state, &m, &undo);

            let mut line = vec![m];
            line.extend(rest);
            Some(line)
        } else {
            // 守り方: 全ての合法手が詰みであることを確認
            let board_moves = legal_board_moves(state);
            let drop_moves = legal_drop_moves(state);

            if board_moves.is_empty() && drop_moves.is_empty() {
                if is_in_check(state, Owner::Defender) {
                    return Some(vec![]); // 詰み
                } else {
                    return None;
                }
            }

            if depth == 0 {
                return None;
            }

            // 無駄合いフィルタ（ドロップ・移動合い両方に適用）
            let all_moves: Vec<Move> = board_moves.into_iter().chain(drop_moves).collect();
            let moves = filter_moves_if_wasteful(state, &all_moves, depth, self);

            if moves.is_empty() {
                if is_in_check(state, Owner::Defender) {
                    return Some(vec![]);
                } else {
                    return None;
                }
            }

            let mut best_line: Option<Vec<Move>> = None;

            for m in &moves {
                let undo = make_move(state, m);
                let ck = tt_key(state, depth - 1);
                let (cpn, _) = self.lookup(ck);

                if cpn != 0 {
                    if cpn < INF {
                        self.mid(state, depth - 1, INF, INF);
                        let ck2 = tt_key(state, depth - 1);
                        let (cpn2, _) = self.lookup(ck2);
                        if cpn2 != 0 {
                            undo_move(state, m, &undo);
                            return None; // 守り方が逃げられる
                        }
                    } else {
                        undo_move(state, m, &undo);
                        return None;
                    }
                }

                let rest = self.extract_unique_solution(state, depth - 1)?;
                undo_move(state, m, &undo);

                if best_line.is_none() || rest.len() + 1 > best_line.as_ref().unwrap().len() {
                    let mut full = vec![m.clone()];
                    full.extend(rest);
                    best_line = Some(full);
                }
            }

            best_line.or_else(|| Some(vec![]))
        }
    }
}

/// 無駄合いフィルタの共通処理
/// スライド駒による単独王手の場合、無駄な合駒（ドロップ・移動合い両方）を除外する
fn filter_moves_if_wasteful(
    state: &mut State,
    moves: &[Move],
    depth: u32,
    solver: &mut DfpnSolver,
) -> Vec<Move> {
    if depth < 2 {
        return moves.to_vec();
    }
    let checkers = find_checkers(state, Owner::Defender);
    if checkers.len() != 1 {
        return moves.to_vec();
    }
    let (ck_pos, ck_bp) = checkers[0];
    if !is_sliding_piece(ck_bp.piece_type) {
        return moves.to_vec();
    }
    let kp = match state.king_pos(Owner::Defender) {
        Some(p) => p,
        None => return moves.to_vec(),
    };

    let mut result = Vec::new();
    for m in moves {
        let to_pos = Pos::new(m.to[0], m.to[1]);
        if is_between(ck_pos, kp, to_pos) {
            // 玉の移動は合駒ではないのでスキップしない
            if let Some(from) = m.from {
                if let Some(bp) = state.get(Pos::new(from[0], from[1])) {
                    if bp.piece_type == PieceType::K {
                        result.push(m.clone());
                        continue;
                    }
                }
            }
            let can_promote = ck_bp.piece_type.is_promotable()
                && (promotion_zone(ck_bp.owner, ck_pos.y) || promotion_zone(ck_bp.owner, to_pos.y));
            let recapture = Move {
                from: Some([ck_pos.x, ck_pos.y]),
                to: [to_pos.x, to_pos.y],
                drop: None,
                promote: can_promote,
            };
            let undo1 = make_move(state, m);
            let undo2 = make_move(state, &recapture);
            let child_key = tt_key(state, depth - 2);
            // 取り返し後に詰むか df-pn で確認
            solver.mid(state, depth - 2, INF, INF);
            let (cpn, _) = solver.lookup(child_key);
            undo_move(state, &recapture, &undo2);
            undo_move(state, m, &undo1);

            if cpn == 0 {
                continue; // 無駄合い → 除外
            }
        }
        result.push(m.clone());
    }
    result
}

/// 王手になる合法手を生成してヒューリスティックでソートする
fn generate_checks(state: &mut State) -> Vec<Move> {
    let enemy = state.side_to_move.opposite();
    let moves = legal_moves(state);
    let mut checks: Vec<Move> = moves
        .into_iter()
        .filter(|m| {
            let undo = make_move(state, m);
            let check = is_in_check(state, enemy);
            undo_move(state, m, &undo);
            check
        })
        .collect();

    // ソート: 捕獲+成り > 捕獲 > 成り > 移動 > 打ち
    checks.sort_by_key(|m| {
        if m.from.is_some() {
            let to = Pos::new(m.to[0], m.to[1]);
            let is_capture = state.get(to).is_some();
            if is_capture && m.promote { 0 }
            else if is_capture { 1 }
            else if m.promote { 2 }
            else { 3 }
        } else {
            4
        }
    });
    checks
}

/// 指定手数ちょうどで詰むかだけ判定する（唯一解チェックなし）
/// 段階的バリデーションの第1段階として使用
pub fn has_mate_at_depth(state: &mut State, mate_length: u32) -> bool {
    if mate_length.is_multiple_of(2) || mate_length == 0 {
        return false;
    }
    if state.side_to_move != Owner::Attacker {
        return false;
    }
    if state.king_pos(Owner::Defender).is_none() {
        return false;
    }
    if is_in_check(state, Owner::Defender) {
        return false;
    }

    let mut solver = DfpnSolver::new();
    solver.solve(state, mate_length)
}

/// より短い手数で詰むかを判定する
/// 段階的バリデーションの第2段階として使用
pub fn has_shorter_mate(state: &mut State, mate_length: u32) -> bool {
    if mate_length <= 1 {
        return false;
    }
    let mut solver = DfpnSolver::new();
    let mut d = 1;
    while d < mate_length {
        if solver.solve(state, d) {
            return true;
        }
        d += 2;
    }
    false
}

/// df-pn による詰将棋検証
/// 既存の validate_tsume_puzzle と同じインターフェースで、内部は df-pn を使用
pub fn validate_tsume_dfpn(state: &mut State, mate_length: u32) -> Option<Vec<Move>> {
    if mate_length.is_multiple_of(2) || mate_length == 0 {
        return None;
    }
    if state.side_to_move != Owner::Attacker {
        return None;
    }
    state.king_pos(Owner::Defender)?;

    if is_in_check(state, Owner::Defender) {
        return None;
    }

    let mut solver = DfpnSolver::new();

    // 反復深化: 短い手数で詰まないことを確認
    let mut d = 1;
    while d < mate_length {
        if solver.solve(state, d) {
            return None; // より短い手数で詰む
        }
        d += 2;
    }

    // 指定手数で詰むか確認
    if !solver.solve(state, mate_length) {
        return None;
    }

    // 解の抽出と唯一解チェック
    solver.extract_unique_solution(state, mate_length)
}

/// 段階的バリデーション: 安い順に検証し、早期棄却する
/// 1. 指定手数で詰むか（唯一解チェックなし）
/// 2. より短い手数で詰まないか
/// 3. 唯一解か（最も重い処理）
pub fn validate_tsume_dfpn_staged(state: &mut State, mate_length: u32) -> Option<Vec<Move>> {
    if mate_length.is_multiple_of(2) || mate_length == 0 {
        return None;
    }
    if state.side_to_move != Owner::Attacker {
        return None;
    }
    state.king_pos(Owner::Defender)?;
    if is_in_check(state, Owner::Defender) {
        return None;
    }

    // 第1段階: 指定手数で詰むか（安い）
    let mut solver = DfpnSolver::new();
    if !solver.solve(state, mate_length) {
        return None;
    }

    // 第2段階: より短い手数で詰まないか
    let mut d = 1;
    while d < mate_length {
        if solver.solve(state, d) {
            return None;
        }
        d += 2;
    }

    // 第3段階: 唯一解チェック（最も重い）
    solver.extract_unique_solution(state, mate_length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shogi::{
        BoardPiece, Owner, PieceType, Pos, State,
    };

    /// テスト用: 簡単な1手詰局面を作成
    fn make_1mate_state() -> State {
        let mut state = State::new();
        state.set(Pos::new(1, 1), Some(BoardPiece {
            owner: Owner::Defender,
            piece_type: PieceType::K,
        }));
        state.hands.add(Owner::Attacker, PieceType::G, 1);
        state.set(Pos::new(2, 1), Some(BoardPiece {
            owner: Owner::Attacker,
            piece_type: PieceType::G,
        }));
        state.set(Pos::new(2, 2), Some(BoardPiece {
            owner: Owner::Attacker,
            piece_type: PieceType::S,
        }));
        state.side_to_move = Owner::Attacker;
        state.compute_zobrist();
        state
    }

    #[test]
    fn test_dfpn_1mate() {
        let mut state = make_1mate_state();
        let mut solver = DfpnSolver::new();
        assert!(solver.solve(&mut state, 1), "1手詰が見つかるべき");
        assert!(!solver.solve(&mut state, 0), "0手では詰まない");
    }

    #[test]
    fn test_dfpn_no_mate() {
        let mut state = State::new();
        state.set(Pos::new(5, 1), Some(BoardPiece {
            owner: Owner::Defender,
            piece_type: PieceType::K,
        }));
        state.hands.add(Owner::Attacker, PieceType::P, 1);
        state.side_to_move = Owner::Attacker;
        state.compute_zobrist();

        let mut solver = DfpnSolver::new().with_node_limit(100_000);
        assert!(!solver.solve(&mut state, 1), "詰まないはず");
        assert!(!solver.solve(&mut state, 3), "詰まないはず");
    }
}
