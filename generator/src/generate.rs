use std::collections::HashSet;
use rustc_hash::FxHashMap;
use std::fs;
use rayon::prelude::*;
use serde::{Serialize, Deserialize};

use shogi_core::shogi::*;
use shogi_core::dfpn;
use shogi_core::rng::Rng;
use crate::backward;
use crate::GenerateMethod;

/// 手数に応じて最適なソルバーを自動選択する
/// 7手以上は df-pn（段階的バリデーション版）、それ未満は従来の全幅探索
fn validate_tsume_auto(state: &mut State, mate_length: u32) -> Option<Vec<Move>> {
    if mate_length >= 7 {
        dfpn::validate_tsume_dfpn_staged(state, mate_length)
    } else {
        validate_tsume_puzzle(state, mate_length)
    }
}

/// validate コマンド用の検証関数（生成時と同じソルバーを使用）
pub fn validate_tsume_for_verify(state: &mut State, mate_length: u32) -> Option<Vec<Move>> {
    validate_tsume_auto(state, mate_length)
}

/// 安価な事前フィルタ: フルバリデーションの前に候補を早期棄却する
fn quick_reject(initial: &InitialData, mate_length: u32) -> bool {
    // 駒数上限チェック（角4枚など不正な局面を棄却）
    if initial.has_excess_pieces() {
        return true;
    }

    let normalized = strip_attacker_king(initial);
    let mut state = normalized.to_state();

    // 守り方の玉がなければ棄却
    let dk = match state.king_pos(Owner::Defender) {
        Some(p) => p,
        None => return true,
    };

    // すでに王手がかかっている（攻め方手番で不正）
    if is_in_check(&state, Owner::Defender) {
        return true;
    }

    // 王手できる手が1つもなければ棄却
    let checks = legal_check_moves(&mut state).len();
    if checks == 0 {
        return true;
    }

    // 王手の手が多すぎると唯一解になりにくい（手数別に閾値を調整）
    let max_checks = match mate_length {
        1..=3 => 20,   // 短手数は比較的緩く
        5 => 15,
        7 => 10,
        9 => 10,
        _ => 10,       // 11手詰以上も分岐を厳しめに抑える
    };
    if checks > max_checks {
        return true;
    }

    // 玉の周囲の空きマスが多すぎると長手数詰みになりにくい
    if mate_length >= 5 {
        let mut empty_around = 0;
        for dx in -1..=1i8 {
            for dy in -1..=1i8 {
                if dx == 0 && dy == 0 { continue; }
                let nx = dk.x + dx;
                let ny = dk.y + dy;
                if (1..=9).contains(&nx) && (1..=9).contains(&ny)
                    && state.get(Pos::new(nx, ny)).is_none() {
                    empty_around += 1;
                }
            }
        }
        let max_empty = match mate_length {
            5 => 6,
            7 => 5,
            9 => 4,
            _ => 4,  // 11手詰以上も玉周りの疎な局面を早めに棄却
        };
        if empty_around >= max_empty {
            return true;
        }
    }

    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Puzzle {
    pub id: u32,
    #[serde(rename = "mateLength")]
    pub mate_length: u32,
    pub initial: InitialData,
    pub solution: Vec<Move>,
    pub quality: String,
    pub score: i32,
    #[serde(default)]
    pub hash: String,
}

/// 初期局面から決定的なハッシュIDを計算する（8文字hex）
pub fn compute_puzzle_hash(initial: &InitialData) -> String {
    // 駒を正規化ソート（owner, x, y, piece_type）して決定的にする
    let mut pieces = initial.pieces.clone();
    pieces.sort_by(|a, b| {
        let oa = if a.owner == Owner::Attacker { 0 } else { 1 };
        let ob = if b.owner == Owner::Attacker { 0 } else { 1 };
        oa.cmp(&ob)
            .then(a.x.cmp(&b.x))
            .then(a.y.cmp(&b.y))
            .then(format!("{:?}", a.piece_type).cmp(&format!("{:?}", b.piece_type)))
    });
    let normalized = InitialData {
        pieces,
        hands: initial.hands.clone(),
        side_to_move: initial.side_to_move,
    };
    let json = serde_json::to_string(&normalized).unwrap_or_default();
    // djb2 ハッシュ
    let mut hash: u64 = 5381;
    for b in json.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    format!("{:08x}", hash as u32)
}

fn empty_hands_data() -> HandsData {
    HandsData {
        attacker: HandCount::default(),
        defender: HandCount::default(),
    }
}

/// HandCount の指定駒種の枚数を設定する
fn set_hand_count(hc: &mut HandCount, t: PieceType, v: u8) {
    match t {
        PieceType::R => hc.R = v,
        PieceType::B => hc.B = v,
        PieceType::G => hc.G = v,
        PieceType::S => hc.S = v,
        PieceType::N => hc.N = v,
        PieceType::L => hc.L = v,
        PieceType::P => hc.P = v,
        _ => {}
    }
}

/// 各駒種の最大枚数 (玉を除く)
fn max_piece_count(t: PieceType) -> u8 {
    match t {
        PieceType::R => 2,
        PieceType::B => 2,
        PieceType::G => 4,
        PieceType::S => 4,
        PieceType::N => 4,
        PieceType::L => 4,
        PieceType::P => 18,
        _ => 0,
    }
}

/// 将棋の駒枚数上限を超えていないか検証する
/// (盤上 + 持ち駒の合計が、各駒種の最大枚数以内であること)
fn piece_count_valid(initial: &InitialData) -> bool {
    let mut counts = FxHashMap::default();
    for p in &initial.pieces {
        let base = p.piece_type.unpromote();
        if base == PieceType::K { continue; }
        *counts.entry(base).or_insert(0u8) += 1;
    }
    // 持ち駒の枚数を加算
    let atk = initial.hands.attacker.to_array();
    let def = initial.hands.defender.to_array();
    for (i, &t) in HAND_TYPES.iter().enumerate() {
        *counts.entry(t).or_insert(0) += atk[i] + def[i];
    }
    counts.iter().all(|(&t, &c)| c <= max_piece_count(t))
}

fn basic_validity(initial: &InitialData) -> bool {
    if initial.pieces.is_empty() { return false; }
    for p in &initial.pieces {
        if p.x < 1 || p.x > 9 || p.y < 1 || p.y > 9 { return false; }
    }

    // Check for duplicate positions
    let mut seen = HashSet::new();
    for p in &initial.pieces {
        if !seen.insert((p.x, p.y)) { return false; }
    }

    // 守り方の玉は必須（攻め方の玉は不要）
    let dk = initial.pieces.iter().find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K);
    if dk.is_none() { return false; }

    if !piece_count_valid(initial) { return false; }
    if initial.has_dead_end_pieces() { return false; }

    true
}

/// スライド駒かどうかを判定（飛・角・香は位置をずらしただけの類似局面を同一視する）
fn is_slider(pt: PieceType) -> bool {
    matches!(pt, PieceType::R | PieceType::B | PieceType::L)
}

/// 局面の構造的シグネチャを計算する（左右反転を正規化して同一視）
/// 守り方玉からの相対座標で表現するため、盤面上の位置が違っても
/// 構造が同じなら同じシグネチャになる
/// スライド駒（飛・角・香）は正確な座標ではなく玉からの方向のみで表現し、
/// 距離をずらしただけの類似パズルを同一視する
pub fn structural_signature(initial: &InitialData) -> String {
    let dk = match initial.pieces.iter().find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K) {
        Some(k) => k,
        None => return String::new(),
    };

    /// 相対座標のリストをソートする
    fn sort_rel(rel: &mut [(Owner, PieceType, i8, i8)]) {
        rel.sort_by(|a, b| {
            format!("{:?}", a.0).cmp(&format!("{:?}", b.0))
                .then(format!("{:?}", a.1).cmp(&format!("{:?}", b.1)))
                .then(a.3.cmp(&b.3))
                .then(a.2.cmp(&b.2))
        });
    }

    /// スライド駒は方向（signum）のみに丸め、非スライド駒はそのまま相対座標
    fn to_rel(p: &PieceData, dk_x: i8, dk_y: i8, mirror: bool) -> (Owner, PieceType, i8, i8) {
        let rx = if mirror { -(p.x - dk_x) } else { p.x - dk_x };
        let ry = p.y - dk_y;
        if is_slider(p.piece_type) {
            // 方向のみ（-1, 0, 1）で表現
            (p.owner, p.piece_type, rx.signum(), ry.signum())
        } else {
            (p.owner, p.piece_type, rx, ry)
        }
    }

    let mut rel_base: Vec<_> = initial.pieces.iter()
        .map(|p| to_rel(p, dk.x, dk.y, false))
        .collect();
    sort_rel(&mut rel_base);

    let mut rel_mirror: Vec<_> = initial.pieces.iter()
        .map(|p| to_rel(p, dk.x, dk.y, true))
        .collect();
    sort_rel(&mut rel_mirror);

    // 左右反転を正規化: 辞書順で小さい方を採用
    let a = format!("{:?}{:?}", rel_base, initial.hands);
    let b = format!("{:?}{:?}", rel_mirror, initial.hands);
    if a < b { a } else { b }
}

/// ランダム候補生成のパラメータ
struct CandidateParams {
    /// 攻め方の駒数 (最小, 最大)
    atk_count: (i8, i8),
    /// 攻め方の駒配置範囲 (守り方玉からの相対x最小, x最大, y最小, y最大)
    atk_range: (i8, i8, i8, i8),
    /// 守り方の駒数 (最小, 最大)
    def_count: (i8, i8),
    /// 守り方の駒配置範囲
    def_range: (i8, i8, i8, i8),
    /// 通常持ち駒を追加する確率
    hand_prob: f64,
    /// 大駒(飛・角)の持ち駒を追加する確率
    major_hand_prob: f64,
    /// 長手数モード（接触王手向き駒種を使用）
    long_mate: bool,
}

/// 駒種配列: 金・銀を多めに含めて出現確率を調整
const ATK_PIECE_TYPES: [PieceType; 9] = [
    PieceType::R, PieceType::B, PieceType::G, PieceType::S,
    PieceType::N, PieceType::L, PieceType::P, PieceType::G, PieceType::S,
];
/// 長手数用: 接触王手向きの駒種を重視（金銀桂を多く、飛角を減らす）
const ATK_PIECE_TYPES_LONG: [PieceType; 10] = [
    PieceType::G, PieceType::S, PieceType::G, PieceType::S,
    PieceType::N, PieceType::P, PieceType::L,
    PieceType::G, PieceType::S, PieceType::R,
];
const DEF_PIECE_TYPES: [PieceType; 9] = [
    PieceType::G, PieceType::S, PieceType::P, PieceType::N,
    PieceType::L, PieceType::G, PieceType::S, PieceType::R, PieceType::B,
];
const HAND_MINOR_TYPES: [PieceType; 5] = [
    PieceType::P, PieceType::S, PieceType::G, PieceType::N, PieceType::L,
];
const HAND_MAJOR_TYPES: [PieceType; 2] = [PieceType::R, PieceType::B];

/// 持ち駒に1枚追加するヘルパー
fn add_hand_piece(hands: &mut HandsData, t: PieceType) {
    match t {
        PieceType::P => hands.attacker.P = 1,
        PieceType::S => hands.attacker.S = 1,
        PieceType::G => hands.attacker.G = 1,
        PieceType::N => hands.attacker.N = 1,
        PieceType::L => hands.attacker.L = 1,
        PieceType::R => hands.attacker.R = 1,
        PieceType::B => hands.attacker.B = 1,
        _ => {}
    }
}

fn piece_can_be_placed(owner: Owner, piece_type: PieceType, y: i8) -> bool {
    match piece_type {
        PieceType::P | PieceType::L => {
            !((owner == Owner::Attacker && y == 1) || (owner == Owner::Defender && y == 9))
        }
        PieceType::N => {
            !((owner == Owner::Attacker && y <= 2) || (owner == Owner::Defender && y >= 8))
        }
        _ => true,
    }
}

/// 守り方玉の周辺にランダムに駒を配置する共通処理
#[allow(clippy::too_many_arguments)]
fn place_pieces_near_king(
    rng: &mut Rng,
    pieces: &mut Vec<PieceData>,
    used: &mut HashSet<(i8, i8)>,
    dk: &PieceData,
    owner: Owner,
    count: usize,
    types: &[PieceType],
    range: (i8, i8, i8, i8),
) {
    for _ in 0..count {
        let t = *rng.pick(types);
        let mut x;
        let mut y;
        let mut guard = 0;
        loop {
            x = (dk.x + rng.ri(range.0, range.1)).clamp(1, 9);
            y = (dk.y + rng.ri(range.2, range.3)).clamp(1, 9);
            guard += 1;
            if (!used.contains(&(x, y)) && piece_can_be_placed(owner, t, y)) || guard >= 40 { break; }
        }
        if used.contains(&(x, y)) || !piece_can_be_placed(owner, t, y) { continue; }
        used.insert((x, y));
        pieces.push(PieceData { x, y, owner, piece_type: t });
    }
}

/// 手数に応じたパラメータでランダム候補局面を生成する
fn random_candidate(rng: &mut Rng, params: &CandidateParams) -> Option<InitialData> {
    let mut pieces = Vec::new();
    let mut used = HashSet::new();

    // 守り方の玉を端寄りに配置（端配置は逃げ道が少なく詰ませやすい）
    let dk_x = if rng.next_f64() < 0.6 {
        // 端寄り: 1,2 または 8,9
        *rng.pick(&[1i8, 2, 8, 9])
    } else {
        rng.ri(3, 7)
    };
    let dk = PieceData { x: dk_x, y: rng.ri(1, 3), owner: Owner::Defender, piece_type: PieceType::K };
    used.insert((dk.x, dk.y));
    pieces.push(dk.clone());

    // 攻め方の駒を配置（長手数では接触王手向き駒種を使用）
    let atk_count = rng.ri(params.atk_count.0, params.atk_count.1) as usize;
    let atk_types: &[PieceType] = if params.long_mate { &ATK_PIECE_TYPES_LONG } else { &ATK_PIECE_TYPES };
    place_pieces_near_king(rng, &mut pieces, &mut used, &dk, Owner::Attacker,
        atk_count, atk_types, params.atk_range);

    // 守り方の駒を配置
    let def_count = rng.ri(params.def_count.0, params.def_count.1) as usize;
    place_pieces_near_king(rng, &mut pieces, &mut used, &dk, Owner::Defender,
        def_count, &DEF_PIECE_TYPES, params.def_range);

    // 持ち駒をランダムに追加
    let mut hands = empty_hands_data();
    if rng.next_f64() < params.hand_prob {
        add_hand_piece(&mut hands, *rng.pick(&HAND_MINOR_TYPES));
    }
    if rng.next_f64() < params.major_hand_prob {
        add_hand_piece(&mut hands, *rng.pick(&HAND_MAJOR_TYPES));
    }

    Some(InitialData { pieces, hands, side_to_move: Owner::Attacker })
}

/// 手数ごとの候補生成パラメータ
fn candidate_params(mate_length: u32) -> CandidateParams {
    match mate_length {
        1 => CandidateParams {
            atk_count: (1, 2), atk_range: (-2, 2, -1, 3),
            def_count: (0, 1), def_range: (-1, 1, -1, 1),
            hand_prob: 0.2, major_hand_prob: 0.0,
            long_mate: false,
        },
        3 => CandidateParams {
            atk_count: (2, 3), atk_range: (-3, 3, -2, 4),
            def_count: (0, 2), def_range: (-2, 2, -1, 2),
            hand_prob: 0.3, major_hand_prob: 0.1,
            long_mate: false,
        },
        5 => CandidateParams {
            atk_count: (2, 4), atk_range: (-3, 3, -1, 4),
            def_count: (1, 3), def_range: (-2, 2, -1, 2),
            hand_prob: 0.4, major_hand_prob: 0.15,
            long_mate: true,
        },
        7 | 9 => CandidateParams {
            atk_count: (3, 5), atk_range: (-3, 3, -1, 5),
            def_count: (1, 3), def_range: (-2, 2, -1, 2),
            hand_prob: 0.5, major_hand_prob: 0.2,
            long_mate: true,
        },
        _ => CandidateParams {
            // 11手以上: より多くの駒、広い配置範囲
            atk_count: (4, 6), atk_range: (-4, 4, -2, 6),
            def_count: (1, 4), def_range: (-3, 3, -1, 3),
            hand_prob: 0.5, major_hand_prob: 0.25,
            long_mate: true,
        },
    }
}

fn mutate_initial(rng: &mut Rng, seed: &InitialData) -> Option<InitialData> {
    let mut cand = seed.clone();
    let ops = ["move-piece", "move-piece", "move-piece", "swap-type", "add-piece", "remove-piece", "tweak-hand"];
    let op = *rng.pick(&ops);

    match op {
        "move-piece" => {
            let movable: Vec<usize> = cand.pieces.iter().enumerate()
                .filter(|(_, p)| p.piece_type != PieceType::K)
                .map(|(i, _)| i)
                .collect();
            if movable.is_empty() { return None; }
            let idx = *rng.pick(&movable);
            let deltas = [-2i8, -1, 1, 2];
            cand.pieces[idx].x += *rng.pick(&deltas);
            cand.pieces[idx].y += *rng.pick(&deltas);
        }
        "swap-type" => {
            let movable: Vec<usize> = cand.pieces.iter().enumerate()
                .filter(|(_, p)| p.piece_type != PieceType::K)
                .map(|(i, _)| i)
                .collect();
            if movable.is_empty() { return None; }
            let idx = *rng.pick(&movable);
            let types = vec![PieceType::R, PieceType::B, PieceType::G, PieceType::S, PieceType::N, PieceType::L, PieceType::P];
            cand.pieces[idx].piece_type = *rng.pick(&types);
        }
        "add-piece" => {
            let owners = [Owner::Attacker, Owner::Defender];
            let owner = *rng.pick(&owners);
            let types = vec![PieceType::R, PieceType::B, PieceType::G, PieceType::S, PieceType::N, PieceType::L, PieceType::P];
            let t = *rng.pick(&types);
            if let Some(dk) = cand.pieces.iter().find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K) {
                let x = (dk.x + rng.ri(-3, 3)).clamp(1, 9);
                let y = (dk.y + rng.ri(-2, 4)).clamp(1, 9);
                cand.pieces.push(PieceData { x, y, owner, piece_type: t });
            }
            if cand.pieces.len() > 10 {
                let removable: Vec<usize> = cand.pieces.iter().enumerate()
                    .filter(|(_, p)| p.piece_type != PieceType::K)
                    .map(|(i, _)| i)
                    .collect();
                if !removable.is_empty() {
                    let idx = *rng.pick(&removable);
                    cand.pieces.remove(idx);
                }
            }
        }
        "remove-piece" => {
            let removable: Vec<usize> = cand.pieces.iter().enumerate()
                .filter(|(_, p)| p.piece_type != PieceType::K)
                .map(|(i, _)| i)
                .collect();
            if removable.len() > 2 {
                let idx = *rng.pick(&removable);
                cand.pieces.remove(idx);
            }
        }
        "tweak-hand" => {
            // ランダムに1種の持ち駒を0か1に変更
            let t = *rng.pick(&HAND_TYPES);
            let v = rng.ri(0, 1) as u8;
            set_hand_count(&mut cand.hands.attacker, t, v);
        }
        _ => {}
    }

    // Clamp coordinates
    for p in &mut cand.pieces {
        p.x = p.x.clamp(1, 9);
        p.y = p.y.clamp(1, 9);
    }

    if basic_validity(&cand) { Some(cand) } else { None }
}

/// 捨て駒の数を数える（攻め方の着手後、守り方がその駒を取る手を指した場合）
fn count_sacrifices(solution: &[Move]) -> i32 {
    let mut count = 0;
    // 攻め方の手(i)の直後の守り方の手(i+1)が同じマスに着手 → 捨て駒
    for i in (0..solution.len().saturating_sub(1)).step_by(2) {
        let atk = &solution[i];
        let def = &solution[i + 1];
        if atk.to == def.to {
            count += 1;
        }
    }
    count
}

fn score_puzzle(initial: &InitialData, solution: &[Move]) -> i32 {
    let atk_types: HashSet<PieceType> = initial.pieces.iter()
        .filter(|p| p.owner == Owner::Attacker && p.piece_type != PieceType::K)
        .map(|p| p.piece_type)
        .collect();
    let piece_count = initial.pieces.len() as i32;
    let attacker_moves: Vec<_> = solution.iter().step_by(2).collect();
    let drop_count = attacker_moves.iter().filter(|m| m.drop.is_some()).count() as i32;
    let promote_count = attacker_moves.iter().filter(|m| m.promote).count() as i32;
    let unique_targets: HashSet<_> = attacker_moves.iter().map(|m| (m.to[0], m.to[1])).collect();
    let sacrifice_count = count_sacrifices(solution);

    atk_types.len() as i32 * 2
        + drop_count * 5
        + promote_count * 3
        + sacrifice_count * 6
        + unique_targets.len() as i32 * 2
        - (piece_count - 8).max(0)
}

/// Compute a difficulty score for ordering puzzles from easy to hard.
/// Higher score = harder puzzle.
fn difficulty_score(initial: &InitialData, solution: &[Move]) -> i32 {
    let h = &initial.hands.attacker;
    let hand_piece_count = h.total() as i32;
    let has_rook_in_hand = (h.R > 0) as i32;
    let has_bishop_in_hand = (h.B > 0) as i32;

    let atk_moves: Vec<_> = solution.iter().step_by(2).collect();
    let drop_count = atk_moves.iter().filter(|m| m.drop.is_some()).count() as i32;
    let promote_count = atk_moves.iter().filter(|m| m.promote).count() as i32;

    let defender_count = initial.pieces.iter()
        .filter(|p| p.owner == Owner::Defender && p.piece_type != PieceType::K)
        .count() as i32;
    let attacker_count = initial.pieces.iter()
        .filter(|p| p.owner == Owner::Attacker && p.piece_type != PieceType::K)
        .count() as i32;

    let sacrifice_count = count_sacrifices(solution);

    hand_piece_count * 10
        + has_rook_in_hand * 8
        + has_bishop_in_hand * 6
        + drop_count * 5
        + promote_count * 3
        + sacrifice_count * 4
        + defender_count * 2
        + attacker_count
}

fn should_run_piece_prune(mate_length: u32) -> bool {
    mate_length <= 7
}

fn prune_initial(initial: &InitialData, mate_length: u32) -> InitialData {
    let mut cur = initial.clone();
    let mut changed = true;

    while changed {
        changed = false;
        let mut order: Vec<(usize, &PieceData)> = cur.pieces.iter().enumerate()
            .filter(|(_, p)| p.piece_type != PieceType::K)
            .collect();
        order.sort_by(|a, b| {
            let a_def = a.1.owner == Owner::Defender;
            let b_def = b.1.owner == Owner::Defender;
            b_def.cmp(&a_def)
                .then((b.1.y - 5).abs().cmp(&(a.1.y - 5).abs()))
        });

        for (i, _) in order {
            let mut cand = cur.clone();
            cand.pieces.remove(i);
            if !basic_validity(&cand) { continue; }
            let mut state = cand.to_state();
            if validate_tsume_auto(&mut state, mate_length).is_some() {
                cur = cand;
                changed = true;
                break;
            }
        }
    }

    cur
}

/// 攻め方の玉を除去する（curated puzzles に含まれている場合の正規化用）
fn strip_attacker_king(initial: &InitialData) -> InitialData {
    InitialData {
        pieces: initial.pieces.iter()
            .filter(|p| !(p.owner == Owner::Attacker && p.piece_type == PieceType::K))
            .cloned()
            .collect(),
        hands: initial.hands.clone(),
        side_to_move: initial.side_to_move,
    }
}

fn validate_and_prune(initial: &InitialData, mate_length: u32) -> Option<(InitialData, Vec<Move>, i32)> {
    // 安価な事前フィルタ
    if quick_reject(initial, mate_length) {
        return None;
    }

    let normalized = strip_attacker_king(initial);
    let mut state = normalized.to_state();
    let solution = validate_tsume_auto(&mut state, mate_length)?;

    // 手順長が正確に mate_length であることを確認（dfpn が短い手順を返すケースを防ぐ）
    if solution.len() != mate_length as usize {
        return None;
    }

    // prune: 駒を減らしても成立するか試す
    let pruned = if should_run_piece_prune(mate_length) {
        prune_initial(&normalized, mate_length)
    } else {
        normalized.clone()
    };
    let pruned_changed = serde_json::to_string(&pruned).unwrap_or_default()
        != serde_json::to_string(&normalized).unwrap_or_default();
    let (final_initial, final_solution) = if pruned_changed {
        let mut pruned_state = pruned.to_state();
        if let Some(pruned_sol) = validate_tsume_auto(&mut pruned_state, mate_length) {
            if pruned_sol.len() == mate_length as usize {
                (InitialData::from_state(&pruned_state), pruned_sol)
            } else {
                (InitialData::from_state(&state), solution)
            }
        } else {
            (InitialData::from_state(&state), solution)
        }
    } else {
        (InitialData::from_state(&state), solution)
    };

    // Remove unused attacker hand pieces; reject if stripping fails
    let (final_initial, final_solution) = strip_unused_hand(final_initial, final_solution, mate_length)?;

    // 最終的な手順長の確認
    if final_solution.len() != mate_length as usize {
        return None;
    }

    // 駒余りなし: 最終局面で攻め方の持ち駒が残っていたらリジェクト
    if has_leftover_pieces(&final_initial, &final_solution) {
        return None;
    }

    // Normalize: mirror to right side if defender king is on the left half
    let (final_initial, final_solution) = normalize_right(final_initial, final_solution);

    // スライド駒を玉に近づける正規化
    let (final_initial, final_solution) = normalize_slider_distance(final_initial, final_solution, mate_length);

    let score = score_puzzle(&final_initial, &final_solution);
    Some((final_initial, final_solution, score))
}

/// 最終局面で攻め方の持ち駒が残っているか判定する（駒余りチェック）
fn has_leftover_pieces(initial: &InitialData, solution: &[Move]) -> bool {
    let mut state = initial.to_state();
    for m in solution {
        state = apply_move(&state, m);
    }
    state.hands.attacker.iter().sum::<u8>() > 0
}

/// 解の手順を再生して使われなかった攻め方の持ち駒を除去し、再検証する。
/// 除去後に問題として成立しない場合は None を返す。
fn strip_unused_hand(mut initial: InitialData, solution: Vec<Move>, mate_length: u32) -> Option<(InitialData, Vec<Move>)> {
    let mut final_state = initial.to_state();
    for m in &solution {
        final_state = apply_move(&final_state, m);
    }
    let remaining = &final_state.hands.attacker;
    if remaining.iter().sum::<u8>() == 0 {
        return Some((initial, solution));
    }
    // 使われなかった持ち駒を初期局面から差し引く
    let mut atk = initial.hands.attacker.to_array();
    for i in 0..7 {
        atk[i] = atk[i].saturating_sub(remaining[i]);
    }
    initial.hands.attacker = HandCount::from_array(&atk);
    // 再検証: 持ち駒を減らすと解が変わる可能性がある
    let mut new_state = initial.to_state();
    validate_tsume_auto(&mut new_state, mate_length)
        .map(|new_sol| (initial, new_sol))
}

/// 玉を盤面の右上に寄せる正規化（筋番号は右が小さい）
/// 表示は右から1,2,...,9なので、x <= 5 が画面右側。
fn normalize_right(initial: InitialData, solution: Vec<Move>) -> (InitialData, Vec<Move>) {
    let dk = initial.pieces.iter()
        .find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K);
    let needs_mirror = match dk {
        Some(k) => k.x > 5,
        None => false,
    };
    if !needs_mirror {
        return (initial, solution);
    }
    let mirrored_init = initial.mirror();
    let mirrored_sol: Vec<Move> = solution.into_iter().map(|m| {
        Move {
            from: m.from.map(|f| [10 - f[0], f[1]]),
            to: [10 - m.to[0], m.to[1]],
            drop: m.drop,
            promote: m.promote,
        }
    }).collect();
    (mirrored_init, mirrored_sol)
}

/// スライド駒（飛・角・香）を玉に近づける正規化
/// 利き線上で玉から TARGET_DIST マス程度の位置に移動する。
/// すでに近い場合や、移動すると詰みが変わる場合はそのまま。
fn normalize_slider_distance(initial: InitialData, solution: Vec<Move>, mate_length: u32) -> (InitialData, Vec<Move>) {
    const TARGET_DIST: i8 = 4; // 玉からこの距離を目標にする

    // 守り方の玉の位置を取得
    let dk = match initial.pieces.iter()
        .find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K)
    {
        Some(k) => (k.x, k.y),
        None => return (initial, solution),
    };

    // 全スライド駒の移動候補を一括計算
    let mut cand = initial.clone();
    let mut any_moved = false;

    // 占有マスを構築（全駒のインデックスと座標）
    let occupied: Vec<(i8, i8)> = cand.pieces.iter().map(|p| (p.x, p.y)).collect();

    for idx in 0..cand.pieces.len() {
        let piece = &cand.pieces[idx];
        if piece.owner != Owner::Attacker {
            continue;
        }

        // スライド駒のみ対象（成り駒は除外）
        let dirs: &[(i8, i8)] = match piece.piece_type {
            PieceType::R => &[(1, 0), (-1, 0), (0, 1), (0, -1)],
            PieceType::B => &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
            PieceType::L => &[(0, -1)], // 香は上方向のみ
            _ => continue,
        };

        let px = piece.x;
        let py = piece.y;
        let cur_dist = (px - dk.0).abs().max((py - dk.1).abs());

        // すでに目標距離以内なら不要
        if cur_dist <= TARGET_DIST {
            continue;
        }

        // 玉に向かう方向を特定
        let dx = (dk.0 - px).signum();
        let dy = (dk.1 - py).signum();

        // この駒のスライド方向と一致するか確認
        if !dirs.iter().any(|&(ddx, ddy)| ddx == dx && ddy == dy) {
            continue;
        }

        // 利き線上で目標位置を計算（間に他の駒がない範囲）
        let mut best_pos = None;
        let mut nx = px + dx;
        let mut ny = py + dy;
        while (1..=9).contains(&nx) && (1..=9).contains(&ny) {
            if occupied.iter().enumerate().any(|(i, &pos)| i != idx && pos == (nx, ny)) {
                break;
            }
            let dist = (nx - dk.0).abs().max((ny - dk.1).abs());
            if dist >= TARGET_DIST {
                best_pos = Some((nx, ny));
            }
            if dist <= TARGET_DIST {
                break;
            }
            nx += dx;
            ny += dy;
        }

        if let Some((new_x, new_y)) = best_pos {
            if new_x != px || new_y != py {
                cand.pieces[idx].x = new_x;
                cand.pieces[idx].y = new_y;
                any_moved = true;
            }
        }
    }

    if !any_moved {
        return (initial, solution);
    }

    // 一括で再検証（1回だけ）
    let mut cand_state = cand.to_state();
    if let Some(new_sol) = validate_tsume_auto(&mut cand_state, mate_length) {
        if new_sol.len() == mate_length as usize && !has_leftover_pieces(&cand, &new_sol) {
            return (cand, new_sol);
        }
    }

    // 検証失敗: 元のまま返す
    (initial, solution)
}

pub fn load_curated(path: &str) -> FxHashMap<u32, Vec<InitialData>> {
    let mut result = FxHashMap::default();
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return result,
    };

    #[derive(Deserialize)]
    struct CuratedFile {
        #[serde(rename = "3", default)]
        three: Vec<InitialData>,
        #[serde(rename = "5", default)]
        five: Vec<InitialData>,
        #[serde(rename = "7", default)]
        seven: Vec<InitialData>,
        #[serde(rename = "9", default)]
        nine: Vec<InitialData>,
    }

    if let Ok(curated) = serde_json::from_str::<CuratedFile>(&content) {
        if !curated.three.is_empty() { result.insert(3, curated.three); }
        if !curated.five.is_empty() { result.insert(5, curated.five); }
        if !curated.seven.is_empty() { result.insert(7, curated.seven); }
        if !curated.nine.is_empty() { result.insert(9, curated.nine); }
    }

    result
}

/// Extract feature vector for a puzzle (used to compute diversity distance)
fn puzzle_features(init: &InitialData, sol: &[Move]) -> Vec<i32> {
    let dk = init.pieces.iter()
        .find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K)
        .map(|p| (p.x, p.y))
        .unwrap_or((5, 2));

    let atk_pieces: Vec<_> = init.pieces.iter()
        .filter(|p| p.owner == Owner::Attacker && p.piece_type != PieceType::K)
        .collect();

    let has_r = atk_pieces.iter().any(|p| p.piece_type == PieceType::R) as i32;
    let has_b = atk_pieces.iter().any(|p| p.piece_type == PieceType::B) as i32;
    let has_g = atk_pieces.iter().any(|p| p.piece_type == PieceType::G) as i32;
    let has_s = atk_pieces.iter().any(|p| p.piece_type == PieceType::S) as i32;
    let has_n = atk_pieces.iter().any(|p| p.piece_type == PieceType::N) as i32;
    let has_l = atk_pieces.iter().any(|p| p.piece_type == PieceType::L) as i32;
    let has_p = atk_pieces.iter().any(|p| p.piece_type == PieceType::P) as i32;

    let h = &init.hands.attacker;
    let hand_r = (h.R > 0) as i32;
    let hand_b = (h.B > 0) as i32;
    let hand_g = (h.G > 0) as i32;
    let hand_s = (h.S > 0) as i32;
    let hand_n = (h.N > 0) as i32;
    let hand_l = (h.L > 0) as i32;
    let hand_p = (h.P > 0) as i32;

    let atk_moves: Vec<_> = sol.iter().step_by(2).collect();
    let has_drop = atk_moves.iter().any(|m| m.drop.is_some()) as i32;
    let has_promote = atk_moves.iter().any(|m| m.promote) as i32;
    let has_sacrifice = (count_sacrifices(sol) > 0) as i32;

    let piece_count = init.pieces.len() as i32;
    let def_count = init.pieces.iter().filter(|p| p.owner == Owner::Defender && p.piece_type != PieceType::K).count() as i32;

    // 玉の位置を領域(3x3)と座標で特徴化
    let dk_region_x = ((dk.0 - 1) / 3) as i32;
    let dk_region_y = ((dk.1 - 1) / 3) as i32;

    // 攻め方の駒の相対位置（玉からの距離で特徴化）
    let atk_rel_sum: i32 = atk_pieces.iter()
        .map(|p| (p.x - dk.0).abs() as i32 + (p.y - dk.1).abs() as i32)
        .sum();

    // 解の最初の手の移動先（玉からの相対座標）
    let first_target_dx = sol.first().map(|m| m.to[0] as i32 - dk.0 as i32).unwrap_or(0);
    let first_target_dy = sol.first().map(|m| m.to[1] as i32 - dk.1 as i32).unwrap_or(0);

    vec![
        has_r * 6, has_b * 6, has_g * 6, has_s * 6, has_n * 6, has_l * 6, has_p * 6,
        hand_r * 6, hand_b * 6, hand_g * 6, hand_s * 6, hand_n * 6, hand_l * 6, hand_p * 6,
        has_drop * 4, has_promote * 4, has_sacrifice * 4,
        piece_count, def_count,
        dk_region_x * 5, dk_region_y * 5,
        dk.0 as i32, dk.1 as i32,
        atk_rel_sum,
        first_target_dx * 3, first_target_dy * 3,
    ]
}

/// Manhattan-like distance between two feature vectors
fn feature_distance(a: &[i32], b: &[i32]) -> i32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum()
}

/// Greedy diversity ordering within a slice of indices into pool.
/// Returns the indices in diversified order.
fn diversify_within(
    indices: &[usize],
    features: &[Vec<i32>],
) -> Vec<usize> {
    if indices.is_empty() { return vec![]; }
    if indices.len() == 1 { return indices.to_vec(); }

    let mut ordered: Vec<usize> = Vec::with_capacity(indices.len());
    let mut used = HashSet::new();

    // Start with the first element
    ordered.push(indices[0]);
    used.insert(indices[0]);

    while ordered.len() < indices.len() {
        let last_feat = &features[*ordered.last().unwrap()];

        let mut best_idx = None;
        let mut best_dist = -1i32;

        for &i in indices {
            if used.contains(&i) { continue; }
            let dist = feature_distance(last_feat, &features[i]);
            if dist > best_dist {
                best_dist = dist;
                best_idx = Some(i);
            }
        }

        match best_idx {
            Some(idx) => {
                ordered.push(idx);
                used.insert(idx);
            }
            None => break,
        }
    }
    ordered
}

/// Reorder puzzles by difficulty tiers, then diversify within each tier.
/// Easier puzzles come first, harder puzzles later.
fn diversify_order(
    pool: Vec<(InitialData, Vec<Move>, i32)>,
) -> Vec<(InitialData, Vec<Move>, i32)> {
    if pool.is_empty() { return vec![]; }

    // Pre-compute features and difficulty scores
    let features: Vec<Vec<i32>> = pool.iter()
        .map(|(init, sol, _)| puzzle_features(init, sol))
        .collect();
    let difficulties: Vec<i32> = pool.iter()
        .map(|(init, sol, _)| difficulty_score(init, sol))
        .collect();

    // Sort indices by difficulty
    let mut indices: Vec<usize> = (0..pool.len()).collect();
    indices.sort_by_key(|&i| difficulties[i]);

    // Split into 4 tiers (quartiles)
    let num_tiers = 4usize;
    let tier_size = indices.len().div_ceil(num_tiers);

    let mut result = Vec::with_capacity(pool.len());
    for tier in indices.chunks(tier_size) {
        let diversified = diversify_within(tier, &features);
        for &idx in &diversified {
            result.push(pool[idx].clone());
        }
    }
    result
}

/// 駒種を1文字の略称に変換する
fn piece_type_char(t: PieceType) -> &'static str {
    match t.unpromote() {
        PieceType::R => "R", PieceType::B => "B", PieceType::G => "G",
        PieceType::S => "S", PieceType::N => "N", PieceType::L => "L",
        PieceType::P => "P", _ => "?",
    }
}

/// 駒構成をキーにする（駒種 + 持ち駒、位置は無視）
/// 同じ駒構成のパズルが多すぎないように制限するために使う
fn composition_key(initial: &InitialData) -> String {
    let sorted_types = |owner: Owner| -> String {
        let mut types: Vec<&str> = initial.pieces.iter()
            .filter(|p| p.owner == owner && p.piece_type != PieceType::K)
            .map(|p| piece_type_char(p.piece_type))
            .collect();
        types.sort();
        types.join("")
    };
    let h = &initial.hands.attacker;
    format!("a:{} d:{} h:R{}B{}G{}S{}N{}L{}P{}",
        sorted_types(Owner::Attacker), sorted_types(Owner::Defender),
        h.R, h.B, h.G, h.S, h.N, h.L, h.P)
}

/// 攻め方の駒構成のみのキー（守り方は無視）
/// 攻め方が同じで守り方だけ違うパズルの連続を抑制するために使う
fn attacker_composition_key(initial: &InitialData) -> String {
    let mut types: Vec<&str> = initial.pieces.iter()
        .filter(|p| p.owner == Owner::Attacker && p.piece_type != PieceType::K)
        .map(|p| piece_type_char(p.piece_type))
        .collect();
    types.sort();
    let h = &initial.hands.attacker;
    format!("a:{} h:R{}B{}G{}S{}N{}L{}P{}",
        types.join(""),
        h.R, h.B, h.G, h.S, h.N, h.L, h.P)
}

type GeneratedPuzzleEntry = (InitialData, Vec<Move>, i32);
type SaveCallback<'a> = dyn Fn(&[GeneratedPuzzleEntry], u32) + 'a;

#[allow(clippy::too_many_arguments)]
pub fn generate_puzzles(seed: u64, mate_length: u32, attempts: u32, curated_seeds: &[InitialData], max: u32, existing: &[Puzzle], method: GenerateMethod, shorter_puzzles: &[Vec<Puzzle>], save_callback: Option<&SaveCallback<'_>>, extend_from: Option<u32>, run_extend: bool, extend_only: bool) -> Vec<Puzzle> {
    let mut sig_set: HashSet<String> = HashSet::new();
    let mut struct_set: HashSet<String> = HashSet::new();
    let mut comp_count: FxHashMap<String, u32> = FxHashMap::default();
    let mut atk_comp_count: FxHashMap<String, u32> = FxHashMap::default();
    let max_per_composition: u32 = 3; // 同一駒構成のパズルは最大3問
    let max_per_atk_composition: u32 = 3; // 攻め方の駒構成が同じパズルは最大3問
    let mut results: Vec<GeneratedPuzzleEntry> = Vec::new();

    // 既存パズルの再検証・取り込み
    let mut kept = 0u32;
    let mut dropped = 0u32;
    for p in existing {
        let mut state = p.initial.to_state();
        let valid = validate_tsume_for_verify(&mut state, mate_length);
        let has_leftover = match &valid {
            Some(sol) => {
                let mut final_state = p.initial.to_state();
                for m in sol {
                    final_state = apply_move(&final_state, m);
                }
                final_state.hands.attacker.iter().sum::<u8>() > 0
            }
            None => true,
        };
        if valid.is_none() || has_leftover {
            dropped += 1;
            continue;
        }
        let sol = valid.unwrap();
        // スライド駒の正規化を既存パズルにも適用
        let (norm_initial, norm_sol) = normalize_slider_distance(p.initial.clone(), sol, mate_length);
        let sig = serde_json::to_string(&norm_initial).unwrap_or_default();
        let ssig = structural_signature(&norm_initial);
        // 正規化で重複になった場合はスキップ
        if sig_set.contains(&sig) || struct_set.contains(&ssig) {
            dropped += 1;
            continue;
        }
        sig_set.insert(sig);
        struct_set.insert(ssig);
        let ckey = composition_key(&norm_initial);
        *comp_count.entry(ckey).or_insert(0) += 1;
        let akey = attacker_composition_key(&norm_initial);
        *atk_comp_count.entry(akey).or_insert(0) += 1;
        results.push((norm_initial, norm_sol, p.score));
        kept += 1;
    }
    if !existing.is_empty() {
        eprintln!("  {}手詰: 既存パズル {}/{} 問を保持 ({}問除外)", mate_length, kept, existing.len(), dropped);
    }

    // 中間結果を保存するヘルパー（既存パズル数以上になってから保存開始、再検証途中の上書き防止）
    let mut last_saved_count = results.len();
    let save_if_changed = |results: &[(InitialData, Vec<Move>, i32)], last: &mut usize, mate_length: u32| {
        if results.len() > *last {
            if let Some(cb) = &save_callback {
                cb(results, mate_length);
            }
            *last = results.len();
        }
    };

    let add_result = |initial: InitialData, _solution: Vec<Move>, _score: i32,
                      sig_set: &mut HashSet<String>, struct_set: &mut HashSet<String>,
                      comp_count: &mut FxHashMap<String, u32>,
                      atk_comp_count: &mut FxHashMap<String, u32>| -> bool {
        let sig = serde_json::to_string(&initial).unwrap_or_default();
        let ssig = structural_signature(&initial);
        if sig_set.contains(&sig) || struct_set.contains(&ssig) { return false; }
        let ckey = composition_key(&initial);
        let count = comp_count.get(&ckey).copied().unwrap_or(0);
        if count >= max_per_composition { return false; }
        let akey = attacker_composition_key(&initial);
        let acount = atk_comp_count.get(&akey).copied().unwrap_or(0);
        if acount >= max_per_atk_composition { return false; }
        sig_set.insert(sig);
        struct_set.insert(ssig);
        *comp_count.entry(ckey).or_insert(0) += 1;
        *atk_comp_count.entry(akey).or_insert(0) += 1;
        true
    };

    if !extend_only {
        // Add curated puzzles
        for initial in curated_seeds {
            if let Some((fin, sol, score)) = validate_and_prune(initial, mate_length) {
                if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count, &mut atk_comp_count) {
                    results.push((fin, sol, score));
                }
            }
        }

        // Mirror curated
        for initial in curated_seeds {
            let mirrored = initial.mirror();
            if basic_validity(&mirrored) {
                if let Some((fin, sol, score)) = validate_and_prune(&mirrored, mate_length) {
                    if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count, &mut atk_comp_count) {
                        results.push((fin, sol, score));
                    }
                }
            }
        }
    }

    // --- 逆算法フェーズ（9手詰以上ではスキップ: 成功率が極端に低いため） ---
    if !extend_only && (method == GenerateMethod::Backward || method == GenerateMethod::Both) {
        let backward_attempts = match method {
            GenerateMethod::Backward => attempts,
            GenerateMethod::Both => attempts / 2, // 半分を逆算法に割り当て
            _ => 0,
        };
        if backward_attempts > 0 {
            eprintln!("  {}手詰: Starting backward generation with {} attempts...", mate_length, backward_attempts);

            let bw_batch_size = 1000u32;
            let bw_batches = backward_attempts.div_ceil(bw_batch_size);

            for batch in 0..bw_batches {
                if results.len() as u32 >= max { break; }

                let batch_start = batch * bw_batch_size;
                let batch_end = (batch_start + bw_batch_size).min(backward_attempts);
                let batch_range: Vec<u32> = (batch_start..batch_end).collect();

                let found: Vec<(InitialData, Vec<Move>, i32)> = batch_range.par_iter()
                    .filter_map(|&i| {
                        let rng_seed = seed.wrapping_add(i as u64).wrapping_mul(6364136223846793005);
                        let cand = backward::backward_candidate(rng_seed, mate_length)?;
                        validate_and_prune(&cand, mate_length)
                    })
                    .collect();

                for (fin, sol, score) in found {
                    if results.len() as u32 >= max { break; }
                    if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count, &mut atk_comp_count) {
                        results.push((fin, sol, score));
                    }
                }

                if batch % 10 == 0 && batch > 0 {
                    eprintln!("  {}手詰 [backward]: {}/{} attempts, {} found", mate_length, batch_end, backward_attempts, results.len());
                }
                save_if_changed(&results, &mut last_saved_count, mate_length);
            }
            eprintln!("  {}手詰: backward phase done, {} found so far", mate_length, results.len());
        }
    }

    // --- 延長法フェーズ（短手数パズルから2手延長、多段チェーン対応）---
    if run_extend {
        for shorter_set in shorter_puzzles {
            if shorter_set.is_empty() { continue; }
            if results.len() as u32 >= max { break; }

            let source_mate = shorter_set[0].mate_length;
            // --extend-from 指定時は該当する手数からの延長のみ実行
            if let Some(ef) = extend_from {
                if source_mate != ef { continue; }
            }
            // 延長する段数を計算（例: 5手詰→9手詰 = 2段延長）
            let extend_steps = ((mate_length - source_mate) / 2) as usize;
            if extend_steps == 0 { continue; }

            let extend_attempts_per_puzzle = 200u32;
            let total_extend = shorter_set.len() as u32 * extend_attempts_per_puzzle;
            eprintln!("  {}手詰: Starting extend phase from {}問 ({}手詰, {}段延長), {} attempts...",
                mate_length, shorter_set.len(), source_mate, extend_steps, total_extend);

            let extend_batch_size = 1000u32;
            let extend_batches = total_extend.div_ceil(extend_batch_size);

            for batch in 0..extend_batches {
                if results.len() as u32 >= max { break; }

            let batch_start = batch * extend_batch_size;
            let batch_end = (batch_start + extend_batch_size).min(total_extend);
            let batch_range: Vec<u32> = (batch_start..batch_end).collect();

            let extend_ok = std::sync::atomic::AtomicU32::new(0);
            let quick_reject_count = std::sync::atomic::AtomicU32::new(0);
            let validate_fail_count = std::sync::atomic::AtomicU32::new(0);
            let validate_ok_count = std::sync::atomic::AtomicU32::new(0);

            let found: Vec<(InitialData, Vec<Move>, i32)> = batch_range.par_iter()
                .filter_map(|&i| {
                    let source_idx = i as usize / extend_attempts_per_puzzle as usize;
                    let source = shorter_set.get(source_idx)?.initial.clone();
                    // 多段延長: extend_steps 回繰り返す
                    let mut current = source;
                    for step in 0..extend_steps {
                        let rng_seed = seed
                            .wrapping_add(i as u64)
                            .wrapping_mul(2654435761)
                            .wrapping_add(step as u64 * 999979);
                        current = backward::extend_candidate(rng_seed, &current)?;
                    }
                    extend_ok.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let rejected = quick_reject(&current, mate_length);
                    if rejected {
                        quick_reject_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        return None;
                    }
                    match validate_and_prune(&current, mate_length) {
                        Some(r) => {
                            validate_ok_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            Some(r)
                        }
                        None => {
                            validate_fail_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            None
                        }
                    }
                })
                .collect();

            eprintln!("    [diag] extend_ok={}, quick_reject={}, validate_fail={}, validate_ok={}",
                extend_ok.load(std::sync::atomic::Ordering::Relaxed),
                quick_reject_count.load(std::sync::atomic::Ordering::Relaxed),
                validate_fail_count.load(std::sync::atomic::Ordering::Relaxed),
                validate_ok_count.load(std::sync::atomic::Ordering::Relaxed));

            for (fin, sol, score) in found {
                if results.len() as u32 >= max { break; }
                if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count, &mut atk_comp_count) {
                    results.push((fin, sol, score));
                }
            }

            if batch % 10 == 0 && batch > 0 {
                eprintln!("  {}手詰 [extend from {}手詰]: {}/{} attempts, {} found",
                    mate_length, source_mate, batch_end, total_extend, results.len());
            }
            save_if_changed(&results, &mut last_saved_count, mate_length);
            }
            eprintln!("  {}手詰: extend from {}手詰 done, {} found so far", mate_length, source_mate, results.len());
        }
    }

    // --- ランダム法フェーズ ---
    let random_attempts = match method {
        GenerateMethod::Random => attempts,
        GenerateMethod::Both => attempts, // ランダム法も全量実行
        GenerateMethod::Backward => 0,
    };

    if !extend_only && random_attempts > 0 {
    eprintln!("  {}手詰: Starting parallel generation with {} attempts...", mate_length, random_attempts);

    // Generate candidates in parallel using rayon
    let batch_size = 1000u32;
    let num_batches = random_attempts.div_ceil(batch_size);

    for batch in 0..num_batches {
        if results.len() as u32 >= max { break; }

        let batch_start = batch * batch_size;
        let batch_end = (batch_start + batch_size).min(random_attempts);
        let batch_range: Vec<u32> = (batch_start..batch_end).collect();

        // Generate and validate candidates in parallel
        let found: Vec<(InitialData, Vec<Move>, i32)> = batch_range.par_iter()
            .filter_map(|&i| {
                let mut rng = Rng::new(seed.wrapping_add(i as u64).wrapping_mul(2654435761));
                let params = candidate_params(mate_length);
                let cand = random_candidate(&mut rng, &params);
                let cand = cand?;
                validate_and_prune(&cand, mate_length)
            })
            .collect();

        for (fin, sol, score) in found {
            if results.len() as u32 >= max { break; }
            if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count, &mut atk_comp_count) {
                results.push((fin, sol, score));
            }
        }

        if (batch + 1) % 5 == 0 || batch + 1 == num_batches {
            eprintln!("  {}手詰: {}/{} attempts, {} found", mate_length, batch_end, random_attempts, results.len());
        }
        save_if_changed(&results, &mut last_saved_count, mate_length);
    }

    // Phase 2: mutate + mirror from found results（並列化）
    if !results.is_empty() {
        let mutate_attempts = random_attempts;
        let snapshot: Vec<InitialData> = results.iter().map(|(init, _, _)| init.clone()).collect();

        let mut_batch_size = 1000u32;
        let mut_batches = mutate_attempts.div_ceil(mut_batch_size);

        for batch in 0..mut_batches {
            if results.len() as u32 >= max { break; }

            let batch_start = batch * mut_batch_size;
            let batch_end = (batch_start + mut_batch_size).min(mutate_attempts);
            let batch_range: Vec<u32> = (batch_start..batch_end).collect();

            let found: Vec<(InitialData, Vec<Move>, i32)> = batch_range.par_iter()
                .filter_map(|&i| {
                    let mut rng = Rng::new(seed.wrapping_add(77777).wrapping_add(i as u64).wrapping_mul(6364136223846793005));
                    let seed_idx = rng.next_u64() as usize % snapshot.len();
                    let seed_initial = &snapshot[seed_idx];
                    // mutation と mirror を交互に試行
                    if i % 5 == 0 {
                        // mirror
                        let mirrored = seed_initial.mirror();
                        if basic_validity(&mirrored) {
                            return validate_and_prune(&mirrored, mate_length);
                        }
                        return None;
                    }
                    let cand = mutate_initial(&mut rng, seed_initial)?;
                    validate_and_prune(&cand, mate_length)
                })
                .collect();

            for (fin, sol, score) in found {
                if results.len() as u32 >= max { break; }
                if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count, &mut atk_comp_count) {
                    results.push((fin, sol, score));
                }
            }

            if (batch + 1) % 5 == 0 || batch + 1 == mut_batches {
                eprintln!("  {}手詰: mutation phase {}/{}, {} found", mate_length, batch_end, mutate_attempts, results.len());
            }
            save_if_changed(&results, &mut last_saved_count, mate_length);
        }
    }
    } // if random_attempts > 0

    // Diversify ordering: greedy "pick the most different puzzle next"
    let final_results = diversify_order(results);

    final_results.iter().enumerate().map(|(i, (init, sol, score))| {
        Puzzle {
            id: i as u32 + 1,
            mate_length,
            initial: init.clone(),
            solution: sol.clone(),
            quality: "validated".to_string(),
            score: *score,
            hash: compute_puzzle_hash(init),
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_hands() -> HandsData {
        HandsData { attacker: HandCount::default(), defender: HandCount::default() }
    }

    fn mk_initial(pieces: Vec<PieceData>) -> InitialData {
        InitialData { pieces, hands: mk_hands(), side_to_move: Owner::Attacker }
    }

    // --- basic_validity テスト ---

    #[test]
    fn test_basic_validity_ok() {
        let init = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        assert!(basic_validity(&init));
    }

    #[test]
    fn test_basic_validity_empty() {
        let init = mk_initial(vec![]);
        assert!(!basic_validity(&init));
    }

    #[test]
    fn test_basic_validity_no_attacker_king() {
        // 攻め方の玉がなくても valid
        let init = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        assert!(basic_validity(&init));
    }

    #[test]
    fn test_basic_validity_no_defender_king() {
        // 守り方の玉がないと invalid
        let init = mk_initial(vec![
            PieceData { x: 5, y: 5, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        assert!(!basic_validity(&init));
    }

    #[test]
    fn test_basic_validity_out_of_bounds() {
        let init = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 0, y: 5, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        assert!(!basic_validity(&init));
    }

    #[test]
    fn test_basic_validity_duplicate_positions() {
        let init = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::G },
            PieceData { x: 3, y: 5, owner: Owner::Defender, piece_type: PieceType::S },
        ]);
        assert!(!basic_validity(&init));
    }

    #[test]
    fn test_basic_validity_dead_end_pieces() {
        let attacker_knight = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 1, y: 2, owner: Owner::Attacker, piece_type: PieceType::N },
        ]);
        assert!(!basic_validity(&attacker_knight));

        let attacker_pawn = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 2, y: 1, owner: Owner::Attacker, piece_type: PieceType::P },
        ]);
        assert!(!basic_validity(&attacker_pawn));

        let defender_knight = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 8, owner: Owner::Defender, piece_type: PieceType::N },
        ]);
        assert!(!basic_validity(&defender_knight));
    }

    // --- piece_count_valid テスト ---

    #[test]
    fn test_piece_count_valid_ok() {
        let init = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::R },
            PieceData { x: 7, y: 5, owner: Owner::Defender, piece_type: PieceType::R },
        ]);
        assert!(piece_count_valid(&init)); // 飛車2枚はOK
    }

    #[test]
    fn test_piece_count_valid_too_many_rooks() {
        let init = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::R },
            PieceData { x: 7, y: 5, owner: Owner::Defender, piece_type: PieceType::R },
            PieceData { x: 1, y: 5, owner: Owner::Attacker, piece_type: PieceType::R },
        ]);
        assert!(!piece_count_valid(&init)); // 飛車3枚はNG
    }

    #[test]
    fn test_piece_count_valid_promoted_counts_as_base() {
        let init = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::PR },
            PieceData { x: 7, y: 5, owner: Owner::Defender, piece_type: PieceType::R },
            PieceData { x: 1, y: 5, owner: Owner::Attacker, piece_type: PieceType::R },
        ]);
        assert!(!piece_count_valid(&init)); // 龍+飛+飛 = 飛車3枚相当でNG
    }

    #[test]
    fn test_piece_count_valid_hand_included() {
        let mut init = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::R },
            PieceData { x: 7, y: 5, owner: Owner::Defender, piece_type: PieceType::R },
        ]);
        init.hands.attacker.R = 1;
        assert!(!piece_count_valid(&init)); // 盤上2 + 持ち駒1 = 飛車3枚でNG
    }

    #[test]
    fn test_piece_count_valid_knights_and_lances() {
        let init = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 1, y: 5, owner: Owner::Attacker, piece_type: PieceType::N },
            PieceData { x: 2, y: 5, owner: Owner::Attacker, piece_type: PieceType::N },
            PieceData { x: 3, y: 5, owner: Owner::Defender, piece_type: PieceType::N },
            PieceData { x: 4, y: 5, owner: Owner::Defender, piece_type: PieceType::N },
        ]);
        assert!(piece_count_valid(&init)); // 桂馬4枚はOK
    }

    // --- structural_signature テスト ---

    #[test]
    fn test_structural_signature_mirror_equal() {
        let init = mk_initial(vec![
            PieceData { x: 3, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 2, y: 2, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        let mirrored = init.mirror();
        assert_eq!(structural_signature(&init), structural_signature(&mirrored));
    }

    #[test]
    fn test_structural_signature_different_positions() {
        let init1 = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 4, y: 2, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        let init2 = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 6, y: 3, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        assert_ne!(structural_signature(&init1), structural_signature(&init2));
    }

    #[test]
    fn test_structural_signature_slider_distance_ignored() {
        // 角を1マスずらしただけの2つの局面は同じシグネチャになるべき
        let init1 = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 3, y: 3, owner: Owner::Attacker, piece_type: PieceType::B },
        ]);
        let init2 = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 2, y: 4, owner: Owner::Attacker, piece_type: PieceType::B },
        ]);
        assert_eq!(structural_signature(&init1), structural_signature(&init2));
    }

    #[test]
    fn test_structural_signature_slider_different_direction() {
        // 角が玉の左下 vs 真下なら別シグネチャ（左右反転で同一視されない方向）
        let init1 = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 3, y: 3, owner: Owner::Attacker, piece_type: PieceType::B },
        ]);
        // 飛車を玉の真下に配置（方向が異なる）
        let init2 = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 3, owner: Owner::Attacker, piece_type: PieceType::B },
        ]);
        assert_ne!(structural_signature(&init1), structural_signature(&init2));
    }

    #[test]
    fn test_structural_signature_non_slider_position_matters() {
        // 金は非スライド駒なので位置が異なれば別シグネチャ
        let init1 = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 4, y: 2, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        let init2 = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 3, y: 3, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        assert_ne!(structural_signature(&init1), structural_signature(&init2));
    }

    // --- difficulty_score テスト ---

    #[test]
    fn test_difficulty_score_no_hand_easier() {
        let init_easy = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 3, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        let sol_easy = vec![Move { from: Some([5, 3]), to: [5, 2], drop: None, promote: false }];

        let mut init_hard = init_easy.clone();
        init_hard.hands.attacker.R = 1;
        let sol_hard = vec![Move { from: None, to: [5, 2], drop: Some(PieceType::R), promote: false }];

        let easy = difficulty_score(&init_easy, &sol_easy);
        let hard = difficulty_score(&init_hard, &sol_hard);
        assert!(hard > easy, "持ち駒飛車ありの方が難易度が高いはず: easy={}, hard={}", easy, hard);
    }

    // --- score_puzzle テスト ---

    #[test]
    fn test_score_puzzle_positive() {
        let init = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 5, y: 3, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        let sol = vec![Move { from: Some([5, 3]), to: [5, 2], drop: None, promote: false }];
        let score = score_puzzle(&init, &sol);
        assert!(score > 0);
    }

    // --- composition_key テスト ---

    #[test]
    fn test_composition_key_same_pieces_same_key() {
        let init1 = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        let init2 = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 7, y: 3, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        assert_eq!(composition_key(&init1), composition_key(&init2));
    }

    #[test]
    fn test_composition_key_different_pieces() {
        let init1 = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::G },
        ]);
        let init2 = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::S },
        ]);
        assert_ne!(composition_key(&init1), composition_key(&init2));
    }

    // --- Rng テスト ---

    #[test]
    fn test_rng_deterministic() {
        let mut rng1 = Rng::new(42);
        let mut rng2 = Rng::new(42);
        for _ in 0..10 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_rng_different_seeds() {
        let mut rng1 = Rng::new(1);
        let mut rng2 = Rng::new(2);
        assert_ne!(rng1.next_u64(), rng2.next_u64());
    }

    // --- random_candidate テスト ---

    #[test]
    fn test_random_candidate_1_generates_valid() {
        let mut rng = Rng::new(42);
        let params = candidate_params(1);
        let mut found = false;
        for _ in 0..100 {
            if let Some(init) = random_candidate(&mut rng, &params) {
                assert!(basic_validity(&init));
                found = true;
                break;
            }
        }
        assert!(found, "100回試行しても一手詰め候補が生成されない");
    }

    #[test]
    fn test_random_candidate_3_generates_valid() {
        let mut rng = Rng::new(42);
        let params = candidate_params(3);
        let mut found = false;
        for _ in 0..100 {
            if let Some(init) = random_candidate(&mut rng, &params) {
                assert!(basic_validity(&init));
                found = true;
                break;
            }
        }
        assert!(found, "100回試行しても三手詰め候補が生成されない");
    }

    #[test]
    fn test_random_candidate_5_generates_valid() {
        let mut rng = Rng::new(42);
        let params = candidate_params(5);
        let mut found = false;
        for _ in 0..100 {
            if let Some(init) = random_candidate(&mut rng, &params) {
                assert!(basic_validity(&init));
                found = true;
                break;
            }
        }
        assert!(found, "100回試行しても五手詰め候補が生成されない");
    }

    // --- mutate_initial テスト ---

    #[test]
    fn test_mutate_produces_valid_or_none() {
        let base = mk_initial(vec![
            PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 3, owner: Owner::Attacker, piece_type: PieceType::G },
            PieceData { x: 7, y: 3, owner: Owner::Attacker, piece_type: PieceType::S },
        ]);
        let mut rng = Rng::new(42);
        for _ in 0..50 {
            if let Some(mutated) = mutate_initial(&mut rng, &base) {
                assert!(basic_validity(&mutated), "mutateの結果がbasic_validityを通らない");
            }
        }
    }

    // --- diversify_order テスト ---

    #[test]
    fn test_diversify_order_keeps_all() {
        let pool: Vec<(InitialData, Vec<Move>, i32)> = (0..10).map(|i| {
            let init = mk_initial(vec![
                PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
                PieceData { x: (i % 8 + 1) as i8, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
                PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::G },
            ]);
            let sol = vec![Move { from: Some([3, 5]), to: [3, 4], drop: None, promote: false }];
            (init, sol, i)
        }).collect();
        let result = diversify_order(pool);
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_diversify_order_empty() {
        let result = diversify_order(vec![]);
        assert!(result.is_empty());
    }

    // --- count_sacrifices テスト ---

    #[test]
    fn test_count_sacrifices_none() {
        // 1手詰（守り方の応手なし）→ 捨て駒なし
        let sol = vec![
            Move { from: Some([3, 3]), to: [3, 2], drop: None, promote: false },
        ];
        assert_eq!(count_sacrifices(&sol), 0);
    }

    #[test]
    fn test_count_sacrifices_one() {
        // 攻め方が(5,3)に打つ → 守り方が(5,3)で取る → 捨て駒1回
        let sol = vec![
            Move { from: None, to: [5, 3], drop: Some(PieceType::G), promote: false },
            Move { from: Some([5, 1]), to: [5, 3], drop: None, promote: false },
            Move { from: Some([3, 2]), to: [4, 2], drop: None, promote: false },
        ];
        assert_eq!(count_sacrifices(&sol), 1);
    }

    #[test]
    fn test_count_sacrifices_not_same_square() {
        // 守り方が別の場所に逃げる → 捨て駒ではない
        let sol = vec![
            Move { from: Some([3, 3]), to: [3, 2], drop: None, promote: false },
            Move { from: Some([5, 1]), to: [4, 1], drop: None, promote: false },
            Move { from: Some([3, 2]), to: [4, 2], drop: None, promote: true },
        ];
        assert_eq!(count_sacrifices(&sol), 0);
    }

    #[test]
    fn test_score_puzzle_includes_sacrifice() {
        let init = mk_initial(vec![
            PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 3, y: 3, owner: Owner::Attacker, piece_type: PieceType::G },
            PieceData { x: 3, y: 2, owner: Owner::Attacker, piece_type: PieceType::S },
        ]);
        // 捨て駒ありの解
        let sol_with = vec![
            Move { from: Some([3, 3]), to: [5, 3], drop: None, promote: false },
            Move { from: Some([5, 1]), to: [5, 3], drop: None, promote: false },
            Move { from: Some([3, 2]), to: [4, 2], drop: None, promote: false },
        ];
        // 捨て駒なしの解
        let sol_without = vec![
            Move { from: Some([3, 3]), to: [4, 2], drop: None, promote: false },
            Move { from: Some([5, 1]), to: [4, 1], drop: None, promote: false },
            Move { from: Some([3, 2]), to: [4, 1], drop: None, promote: false },
        ];
        let score_with = score_puzzle(&init, &sol_with);
        let score_without = score_puzzle(&init, &sol_without);
        assert!(score_with > score_without, "捨て駒ありの方がスコアが高いはず");
    }

    /// 延長法候補の quick_reject 通過率と validate 成功率を診断
    #[test]
    #[ignore] // 長時間かかるため手動実行のみ
    fn test_extend_11mate_rejection_diagnostic() {
        use crate::backward;

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../puzzles/9.json");
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => { eprintln!("puzzles/9.json なし、スキップ"); return; }
        };
        let puzzles: Vec<serde_json::Value> = serde_json::from_str(&data).unwrap();
        eprintln!("9手詰パズル数: {}", puzzles.len());

        let mut extend_ok = 0;
        let mut rejected = 0;
        let mut passed_reject = 0;
        let mate_length = 11;

        // 各パズルから延長候補を生成して quick_reject のみ判定
        for (pi, puzzle) in puzzles.iter().enumerate() {
            let initial: InitialData = serde_json::from_value(puzzle["initial"].clone()).unwrap();
            for attempt in 0..100 {
                let rng_seed = (pi * 1000 + attempt) as u64 + 1;
                if let Some(candidate) = backward::extend_candidate(rng_seed, &initial) {
                    extend_ok += 1;
                    if quick_reject(&candidate, mate_length) {
                        rejected += 1;
                    } else {
                        passed_reject += 1;
                    }
                }
            }
        }

        eprintln!("=== quick_reject 診断 (11手詰延長候補) ===");
        eprintln!("延長候補生成: {}", extend_ok);
        eprintln!("quick_reject 棄却: {} ({:.1}%)", rejected, rejected as f64 / extend_ok as f64 * 100.0);
        eprintln!("quick_reject 通過: {} ({:.1}%)", passed_reject, passed_reject as f64 / extend_ok as f64 * 100.0);

        // quick_reject を通過したものの中から少数だけ validate_and_prune を試す
        let mut validated = 0;
        let mut validate_tried = 0;
        for (pi, puzzle) in puzzles.iter().enumerate() {
            if validate_tried >= 20 { break; }
            let initial: InitialData = serde_json::from_value(puzzle["initial"].clone()).unwrap();
            for attempt in 0..100 {
                if validate_tried >= 20 { break; }
                let rng_seed = (pi * 1000 + attempt) as u64 + 1;
                if let Some(candidate) = backward::extend_candidate(rng_seed, &initial) {
                    if !quick_reject(&candidate, mate_length) {
                        validate_tried += 1;
                        if validate_and_prune(&candidate, mate_length).is_some() {
                            validated += 1;
                        }
                    }
                }
            }
        }
        eprintln!("validate_and_prune 試行: {}, 成功: {}", validate_tried, validated);
    }

    /// quick_reject の通過率を再計測（閾値変更後）
    #[test]
    fn test_extend_11mate_reject_breakdown() {
        use crate::backward;

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../puzzles/9.json");
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => { eprintln!("puzzles/9.json なし、スキップ"); return; }
        };
        let puzzles: Vec<serde_json::Value> = serde_json::from_str(&data).unwrap();

        let mut total = 0u32;
        let mut rejected = 0u32;
        let mut passed = 0u32;

        for (pi, puzzle) in puzzles.iter().enumerate() {
            let initial: InitialData = serde_json::from_value(puzzle["initial"].clone()).unwrap();
            for attempt in 0..100 {
                let rng_seed = (pi * 1000 + attempt) as u64 + 1;
                let candidate = match backward::extend_candidate(rng_seed, &initial) {
                    Some(c) => c,
                    None => continue,
                };
                total += 1;
                if quick_reject(&candidate, 11) {
                    rejected += 1;
                } else {
                    passed += 1;
                }
            }
        }

        eprintln!("=== quick_reject 通過率 (11手詰, 閾値変更後) ===");
        eprintln!("延長候補総数: {}", total);
        eprintln!("棄却: {} ({:.1}%)", rejected, rejected as f64 / total as f64 * 100.0);
        eprintln!("通過: {} ({:.1}%)", passed, passed as f64 / total as f64 * 100.0);
    }
}
