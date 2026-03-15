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
//!
//! 証明駒・反証駒による高速化:
//! - 証明駒: 詰みを証明するのに必要な攻め方の最小持ち駒セット
//! - 反証駒: 不詰を証明するのに必要な攻め方の最小持ち駒セット
//! - 盤面が同一で持ち駒が証明駒以上なら即座に詰みと判定（優等局面）
//! - 盤面が同一で持ち駒が反証駒以下なら即座に不詰と判定（劣等局面）

use crate::shogi::*;
use rustc_hash::FxHashMap;

const INF: u32 = u32::MAX / 2;

/// デフォルトのノード上限
const DEFAULT_NODE_LIMIT: u64 = 50_000_000;

/// 持ち駒の支配関係の最大値（全駒種の最大枚数）
const HAND_MAX: [u8; 7] = [2, 2, 4, 4, 4, 4, 18]; // R, B, G, S, N, L, P

/// 駒種から持ち駒インデックス (0..7) を返す
fn hand_idx(t: PieceType) -> usize {
    match t {
        PieceType::R => 0, PieceType::B => 1, PieceType::G => 2,
        PieceType::S => 3, PieceType::N => 4, PieceType::L => 5,
        PieceType::P => 6, _ => panic!("not a hand type"),
    }
}

/// 持ち駒の支配関係: a の全駒種が b 以上か
fn hand_dominates(a: &[u8; 7], b: &[u8; 7]) -> bool {
    a[0] >= b[0] && a[1] >= b[1] && a[2] >= b[2] && a[3] >= b[3]
        && a[4] >= b[4] && a[5] >= b[5] && a[6] >= b[6]
}

/// 持ち駒の和集合（各駒種の最大値）
fn hand_union(a: &[u8; 7], b: &[u8; 7]) -> [u8; 7] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2]), a[3].max(b[3]),
     a[4].max(b[4]), a[5].max(b[5]), a[6].max(b[6])]
}

/// 支配性テーブルのキー: 盤面ハッシュ（持ち駒除外）+ depth
fn dom_key(state: &State, depth: u32) -> u64 {
    state.board_only_zobrist().wrapping_mul(0x517CC1B727220A95).wrapping_add(depth as u64)
}

/// 支配性テーブルのエントリ
#[derive(Clone)]
struct DomEntry {
    /// 証明駒（この持ち駒以上なら詰み）。None = 未証明
    proof: Option<[u8; 7]>,
    /// 反証駒（この持ち駒以下なら不詰）。None = 未反証
    disproof: Option<[u8; 7]>,
}

/// 支配性テーブルのエントリ上限（1キーあたり）
const DOM_MAX_ENTRIES: usize = 4;

/// トランスポジションテーブルのエントリ
#[derive(Clone)]
struct TtEntry {
    pn: u32,
    dn: u32,
    /// 証明駒（pn==0 のときのみ有効）
    proof: [u8; 7],
    /// 反証駒（dn==0 のときのみ有効）
    disproof: [u8; 7],
}

/// TT キー: Zobrist ハッシュに残り手数を混ぜる
fn tt_key(state: &State, depth: u32) -> u64 {
    state.zobrist_hash.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(depth as u64)
}

/// df-pn ソルバー
pub struct DfpnSolver {
    tt: FxHashMap<u64, TtEntry>,
    /// 支配性テーブル: 盤面+depth → 証明駒/反証駒リスト
    dom: FxHashMap<u64, Vec<DomEntry>>,
    node_count: u64,
    node_limit: u64,
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
            dom: FxHashMap::default(),
            node_count: 0,
            node_limit: DEFAULT_NODE_LIMIT,
        }
    }

    pub fn with_node_limit(mut self, limit: u64) -> Self {
        self.node_limit = limit;
        self
    }

    /// ノード上限を変更する（TTは保持したまま）
    pub fn set_node_limit(&mut self, limit: u64) {
        self.node_limit = limit;
    }

    pub fn node_count(&self) -> u64 {
        self.node_count
    }

    fn lookup(&self, key: u64) -> (u32, u32) {
        match self.tt.get(&key) {
            Some(e) => (e.pn, e.dn),
            None => (1, 1), // 未展開ノードの初期値
        }
    }

    /// TT から証明駒を取得する（pn==0 のとき有効）
    fn lookup_proof(&self, key: u64) -> Option<[u8; 7]> {
        self.tt.get(&key).and_then(|e| if e.pn == 0 { Some(e.proof) } else { None })
    }

    /// TT から反証駒を取得する（dn==0 のとき有効）
    fn lookup_disproof(&self, key: u64) -> Option<[u8; 7]> {
        self.tt.get(&key).and_then(|e| if e.dn == 0 { Some(e.disproof) } else { None })
    }

    fn store(&mut self, key: u64, pn: u32, dn: u32) {
        self.store_with_pieces(key, pn, dn, [0; 7], HAND_MAX);
    }

    fn store_with_pieces(&mut self, key: u64, pn: u32, dn: u32, proof: [u8; 7], disproof: [u8; 7]) {
        self.tt.insert(key, TtEntry { pn, dn, proof, disproof });
    }

    /// 支配性テーブルに証明駒/反証駒を登録する
    fn register_dominance(&mut self, state: &State, depth: u32, proof: Option<[u8; 7]>, disproof: Option<[u8; 7]>) {
        if proof.is_none() && disproof.is_none() {
            return;
        }
        let dk = dom_key(state, depth);
        let entries = self.dom.entry(dk).or_default();
        if entries.len() < DOM_MAX_ENTRIES {
            entries.push(DomEntry { proof, disproof });
        }
    }

    /// 支配性テーブルで詰み/不詰を判定する
    /// 戻り値: Some((pn, dn, proof, disproof)) なら即判定可能
    fn check_dominance(&self, state: &State, depth: u32) -> Option<(u32, u32, [u8; 7], [u8; 7])> {
        let dk = dom_key(state, depth);
        let entries = self.dom.get(&dk)?;
        let hand = &state.hands.attacker;
        for e in entries {
            // 証明駒チェック: 現在の持ち駒 >= 証明駒 → 詰み
            if let Some(ref pp) = e.proof {
                if hand_dominates(hand, pp) {
                    return Some((0, INF, *pp, HAND_MAX));
                }
            }
            // 反証駒チェック: 現在の持ち駒 <= 反証駒 → 不詰
            if let Some(ref dp) = e.disproof {
                if hand_dominates(dp, hand) {
                    return Some((INF, 0, [0; 7], *dp));
                }
            }
        }
        None
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

        // 支配性テーブルによる早期判定
        if let Some((dom_pn, dom_dn, proof, disproof)) = self.check_dominance(state, depth) {
            self.store_with_pieces(key, dom_pn, dom_dn, proof, disproof);
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
            self.store_with_pieces(key, INF, 0, [0; 7], state.hands.attacker);
            self.register_dominance(state, depth, None, Some(state.hands.attacker));
            return;
        }

        // 1手詰ルーチン: depth==1 は専用コードで即判定
        if depth == 1 {
            if is_mate_in_1(state) {
                // 証明駒 = 現在の攻め方持ち駒（簡易版。本来は詰み手から逆算すべき）
                let pp = state.hands.attacker;
                self.store_with_pieces(key, 0, INF, pp, HAND_MAX);
                self.register_dominance(state, depth, Some(pp), None);
            } else {
                let dp = state.hands.attacker;
                self.store_with_pieces(key, INF, 0, [0; 7], dp);
                self.register_dominance(state, depth, None, Some(dp));
            }
            return;
        }

        if depth == 3 {
            if is_mate_in_3(state) {
                let pp = state.hands.attacker;
                self.store_with_pieces(key, 0, INF, pp, HAND_MAX);
                self.register_dominance(state, depth, Some(pp), None);
            } else {
                let dp = state.hands.attacker;
                self.store_with_pieces(key, INF, 0, [0; 7], dp);
                self.register_dominance(state, depth, None, Some(dp));
            }
            return;
        }

        let checks = generate_checks(state);
        if checks.is_empty() {
            let dp = state.hands.attacker;
            self.store_with_pieces(key, INF, 0, [0; 7], dp);
            self.register_dominance(state, depth, None, Some(dp));
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

            // 証明完了: 証明駒を計算して登録
            if min_pn == 0 {
                let pp = self.compute_or_proof(state, &checks, &child_pn, depth);
                self.store_with_pieces(key, 0, sum_dn, pp, HAND_MAX);
                self.register_dominance(state, depth, Some(pp), None);
                return;
            }
            // 反証完了: 反証駒を計算して登録
            if sum_dn == 0 {
                let dp = self.compute_or_disproof(state, &checks, depth);
                self.store_with_pieces(key, min_pn, 0, [0; 7], dp);
                self.register_dominance(state, depth, None, Some(dp));
                return;
            }

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

    /// OR ノードの証明駒を計算する
    /// 証明駒 = best child の証明駒を手の効果で逆算
    fn compute_or_proof(&self, state: &mut State, checks: &[Move], child_pn: &[u32], depth: u32) -> [u8; 7] {
        // pn==0 の最初の子を見つける
        for (i, m) in checks.iter().enumerate() {
            if child_pn[i] != 0 {
                continue;
            }
            let undo = make_move(state, m);
            let ck = tt_key(state, depth - 1);
            let child_pp = self.lookup_proof(ck).unwrap_or(state.hands.attacker);
            undo_move(state, m, &undo);

            // 手の効果を逆算して親ノードの証明駒を計算
            let mut pp = child_pp;
            if let Some(drop_type) = m.drop {
                // 打ち: その駒が必要
                let idx = hand_idx(drop_type);
                pp[idx] = pp[idx].saturating_add(1);
            }
            if let Some(ref cap) = undo.captured {
                // 取り: 取得した駒は事前に不要
                let cap_base = cap.piece_type.unpromote();
                if cap_base != PieceType::K {
                    let idx = hand_idx(cap_base);
                    pp[idx] = pp[idx].saturating_sub(1);
                }
            }
            return pp;
        }
        // フォールバック: 現在の持ち駒をそのまま返す
        state.hands.attacker
    }

    /// OR ノードの反証駒を計算する
    /// 反証駒 = 全子ノードの反証駒の共通部分（各駒種の最小値）を手の効果で逆算
    fn compute_or_disproof(&self, state: &mut State, checks: &[Move], depth: u32) -> [u8; 7] {
        let mut result = HAND_MAX;
        for m in checks {
            let undo = make_move(state, m);
            let ck = tt_key(state, depth - 1);
            let child_dp = self.lookup_disproof(ck).unwrap_or(state.hands.attacker);
            undo_move(state, m, &undo);

            // 手の効果を逆算
            let mut dp = child_dp;
            if let Some(drop_type) = m.drop {
                let idx = hand_idx(drop_type);
                dp[idx] = dp[idx].saturating_add(1);
            }
            if let Some(ref cap) = undo.captured {
                let cap_base = cap.piece_type.unpromote();
                if cap_base != PieceType::K {
                    let idx = hand_idx(cap_base);
                    dp[idx] = dp[idx].saturating_sub(1);
                }
            }
            // 共通部分（各駒種の最小値）
            for j in 0..7 {
                result[j] = result[j].min(dp[j]);
            }
        }
        result
    }

    /// AND ノード（守り方の手番）: 無駄合い判定付き
    fn mid_and(&mut self, state: &mut State, depth: u32, th_pn: u32, th_dn: u32, key: u64) {
        let board_moves = legal_board_moves(state);

        if board_moves.is_empty() {
            let drop_moves = legal_drop_moves(state);
            if drop_moves.is_empty() {
                if is_in_check(state, Owner::Defender) {
                    // 詰み: 証明駒 = 現在の攻め方持ち駒（末端なので正確）
                    let pp = state.hands.attacker;
                    self.store_with_pieces(key, 0, INF, pp, HAND_MAX);
                    self.register_dominance(state, depth, Some(pp), None);
                } else {
                    self.store(key, INF, 0);
                }
                return;
            }
        }

        if depth == 0 {
            let dp = state.hands.attacker;
            self.store_with_pieces(key, INF, 0, [0; 7], dp);
            self.register_dominance(state, depth, None, Some(dp));
            return;
        }

        let drop_moves = legal_drop_moves(state);

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
                if d == 0 {
                    let dp = self.lookup_disproof(ck).unwrap_or(state.hands.attacker);
                    undo_move(state, m, &undo);
                    self.store_with_pieces(key, INF, 0, [0; 7], dp);
                    self.register_dominance(state, depth, None, Some(dp));
                    return;
                }
                undo_move(state, m, &undo);
                child_pn[i] = p;
                child_dn[i] = d;
            }

            let (sum_pn, min_dn) = Self::and_aggregate(&child_pn, &child_dn);

            // 証明完了: 証明駒 = 全子ノードの証明駒の和集合
            if min_dn == INF && sum_pn == 0 {
                let pp = self.compute_and_proof(state, &moves, depth);
                self.store_with_pieces(key, 0, INF, pp, HAND_MAX);
                self.register_dominance(state, depth, Some(pp), None);
                return;
            }
            // 反証完了: 反証駒 = best child の反証駒
            if min_dn == 0 {
                let dp = self.compute_and_disproof(state, &moves, &child_dn, depth);
                self.store_with_pieces(key, sum_pn, 0, [0; 7], dp);
                self.register_dominance(state, depth, None, Some(dp));
                return;
            }

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

    /// AND ノードの証明駒を計算する
    /// 証明駒 = 全子ノードの証明駒の和集合（各駒種の最大値）
    /// 守り方の手は攻め方の持ち駒を変えないので逆算不要
    fn compute_and_proof(&self, state: &mut State, moves: &[Move], depth: u32) -> [u8; 7] {
        let mut result = [0u8; 7];
        for m in moves {
            let undo = make_move(state, m);
            let ck = tt_key(state, depth - 1);
            let child_pp = self.lookup_proof(ck).unwrap_or(state.hands.attacker);
            undo_move(state, m, &undo);
            result = hand_union(&result, &child_pp);
        }
        result
    }

    /// AND ノードの反証駒を計算する
    /// 反証駒 = dn==0 の子ノードの反証駒
    fn compute_and_disproof(&self, state: &mut State, moves: &[Move], child_dn: &[u32], depth: u32) -> [u8; 7] {
        for (i, m) in moves.iter().enumerate() {
            if child_dn[i] != 0 {
                continue;
            }
            let undo = make_move(state, m);
            let ck = tt_key(state, depth - 1);
            let dp = self.lookup_disproof(ck).unwrap_or(state.hands.attacker);
            undo_move(state, m, &undo);
            return dp;
        }
        state.hands.attacker
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

            if board_moves.is_empty() {
                let drop_moves = legal_drop_moves(state);
                if drop_moves.is_empty() {
                    if is_in_check(state, Owner::Defender) {
                        return Some(vec![]); // 詰み
                    } else {
                        return None;
                    }
                }
            }

            if depth == 0 {
                return None;
            }

            let mut best_line: Option<Vec<Move>> = None;

            let board_moves = filter_moves_if_wasteful(state, &board_moves, depth, self);
            for m in &board_moves {
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

            let drop_moves = legal_drop_moves(state);
            let drop_moves = filter_moves_if_wasteful(state, &drop_moves, depth, self);
            if board_moves.is_empty() && drop_moves.is_empty() {
                if is_in_check(state, Owner::Defender) {
                    return Some(vec![]);
                } else {
                    return None;
                }
            }

            for m in &drop_moves {
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
                            return None;
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

/// 1手詰ルーチン: df-pn を使わず直接判定する
/// 攻め方の手番で、王手になる手のうち守り方に合法手がなくなるものがあれば詰み
fn is_mate_in_1(state: &mut State) -> bool {
    let checks = generate_checks(state);
    for m in &checks {
        let undo = make_move(state, m);
        let has_response = has_any_legal_move(state);
        undo_move(state, m, &undo);
        if !has_response {
            return true;
        }
    }
    false
}

/// 3手詰ルーチン: 攻め方の王手 → 守り方の応手 → 1手詰 で判定する
/// 全ての守り方の応手に対して1手詰が成立すれば3手詰
fn is_mate_in_3(state: &mut State) -> bool {
    let checks = generate_checks(state);
    for m in &checks {
        let undo = make_move(state, m);
        let mut all_mated = true;
        let board_moves = legal_board_moves(state);
        if board_moves.is_empty() {
            let drop_moves = legal_drop_moves(state);
            if drop_moves.is_empty() {
                undo_move(state, m, &undo);
                continue;
            }
            for dm in &drop_moves {
                let undo2 = make_move(state, dm);
                if !is_mate_in_1(state) {
                    all_mated = false;
                    undo_move(state, dm, &undo2);
                    break;
                }
                undo_move(state, dm, &undo2);
            }
            undo_move(state, m, &undo);
            if all_mated {
                return true;
            }
            continue;
        }
        for dm in &board_moves {
            let undo2 = make_move(state, dm);
            if !is_mate_in_1(state) {
                all_mated = false;
                undo_move(state, dm, &undo2);
                break;
            }
            undo_move(state, dm, &undo2);
        }
        if all_mated {
            let drop_moves = legal_drop_moves(state);
            for dm in &drop_moves {
                let undo2 = make_move(state, dm);
                if !is_mate_in_1(state) {
                    all_mated = false;
                    undo_move(state, dm, &undo2);
                    break;
                }
                undo_move(state, dm, &undo2);
            }
        }
        undo_move(state, m, &undo);
        if all_mated {
            return true;
        }
    }
    false
}

/// 王手になる合法手を生成してヒューリスティックでソートする
fn generate_checks(state: &mut State) -> Vec<Move> {
    legal_check_moves(state)
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
    if state_has_dead_end_pieces(state) {
        return None;
    }

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
/// 1. 指定手数で詰むか（低ノード上限で高速判定）
/// 2. より短い手数で詰まないか
/// 3. 唯一解か（フルノード上限で実行）
pub fn validate_tsume_dfpn_staged(state: &mut State, mate_length: u32) -> Option<Vec<Move>> {
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
    if is_in_check(state, Owner::Defender) {
        return None;
    }

    // 第1段階: 指定手数で詰むか（中程度のノード上限で判定）
    // 不詰候補の大半をここで棄却する
    let stage1_limit = match mate_length {
        1..=5 => 500_000,
        7 => 2_000_000,
        _ => 5_000_000, // 9手詰以上
    };
    let mut solver = DfpnSolver::new().with_node_limit(stage1_limit);
    if !solver.solve(state, mate_length) {
        return None;
    }

    // 第2段階: より短い手数で詰まないか（中程度のノード上限）
    // 注: dfpn の短手数判定は不完全な場合があるため、validate_and_prune で手順長も別途確認
    solver.set_node_limit(1_000_000);
    let mut d = 1;
    while d < mate_length {
        if solver.solve(state, d) {
            return None;
        }
        d += 2;
    }

    // 第3段階: 唯一解チェック（フルノード上限）
    solver.set_node_limit(DEFAULT_NODE_LIMIT);
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
    fn test_is_mate_in_1() {
        let mut state = make_1mate_state();
        assert!(is_mate_in_1(&mut state), "1手詰ルーチンで詰みを検出");
    }

    #[test]
    fn test_is_mate_in_1_no_mate() {
        let mut state = State::new();
        state.set(Pos::new(5, 5), Some(BoardPiece {
            owner: Owner::Defender,
            piece_type: PieceType::K,
        }));
        state.hands.add(Owner::Attacker, PieceType::P, 1);
        state.side_to_move = Owner::Attacker;
        state.compute_zobrist();
        assert!(!is_mate_in_1(&mut state), "詰まない局面では false");
    }

    #[test]
    fn test_is_mate_in_3() {
        // 3手詰局面: 1二玉、持ち駒 金金
        let mut state = State::new();
        state.set(Pos::new(1, 2), Some(BoardPiece {
            owner: Owner::Defender,
            piece_type: PieceType::K,
        }));
        state.hands.add(Owner::Attacker, PieceType::G, 2);
        state.side_to_move = Owner::Attacker;
        state.compute_zobrist();

        let mut solver = DfpnSolver::new();
        let is_3mate = solver.solve(&mut state, 3);
        if is_3mate {
            assert!(is_mate_in_3(&mut state), "3手詰ルーチンでも詰みを検出");
        }
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

    #[test]
    fn test_proof_pieces_dominance() {
        // 1手詰局面を作成し、持ち駒を増やしても詰むことを確認
        let mut state = make_1mate_state();
        let mut solver = DfpnSolver::new();

        // 通常の持ち駒で詰みを確認
        assert!(solver.solve(&mut state, 1));

        // 持ち駒を増やした局面を作成
        let mut state2 = state.clone();
        state2.hands.add(Owner::Attacker, PieceType::P, 3);
        state2.zobrist_hash = state2.compute_zobrist();

        // 支配性テーブルにより即座に詰みと判定されるはず
        assert!(solver.solve(&mut state2, 1));
    }

    #[test]
    fn test_hand_dominates() {
        assert!(hand_dominates(&[1, 1, 1, 1, 1, 1, 1], &[0, 0, 0, 0, 0, 0, 0]));
        assert!(hand_dominates(&[1, 1, 1, 1, 1, 1, 1], &[1, 1, 1, 1, 1, 1, 1]));
        assert!(!hand_dominates(&[0, 1, 1, 1, 1, 1, 1], &[1, 0, 0, 0, 0, 0, 0]));
    }
}
