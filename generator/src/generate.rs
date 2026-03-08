use std::collections::{HashMap, HashSet};
use std::fs;
use rayon::prelude::*;
use serde::{Serialize, Deserialize};

use crate::shogi::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Puzzle {
    pub id: u32,
    #[serde(rename = "mateLength")]
    pub mate_length: u32,
    pub initial: InitialData,
    pub solution: Vec<Move>,
    pub quality: String,
    pub score: i32,
}

struct Rng {
    x: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Rng { x: if seed == 0 { 123456789 } else { seed } }
    }

    fn next(&mut self) -> u64 {
        self.x ^= self.x << 13;
        self.x ^= self.x >> 7;
        self.x ^= self.x << 17;
        self.x
    }

    fn next_f64(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
    }

    fn ri(&mut self, min: i8, max: i8) -> i8 {
        let range = (max - min + 1) as u64;
        (self.next() % range) as i8 + min
    }

    fn pick<'a, T>(&mut self, arr: &'a [T]) -> &'a T {
        let idx = self.next() as usize % arr.len();
        &arr[idx]
    }
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
    let mut counts = HashMap::new();
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

    true
}

/// 局面の構造的シグネチャを計算する（左右反転を正規化して同一視）
/// 守り方玉からの相対座標で表現するため、盤面上の位置が違っても
/// 構造が同じなら同じシグネチャになる
fn structural_signature(initial: &InitialData) -> String {
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

    let mut rel_base: Vec<_> = initial.pieces.iter()
        .map(|p| (p.owner, p.piece_type, p.x - dk.x, p.y - dk.y))
        .collect();
    sort_rel(&mut rel_base);

    let mut rel_mirror: Vec<_> = initial.pieces.iter()
        .map(|p| (p.owner, p.piece_type, -(p.x - dk.x), p.y - dk.y))
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
}

/// 駒種配列: 金・銀を多めに含めて出現確率を調整
const ATK_PIECE_TYPES: [PieceType; 9] = [
    PieceType::R, PieceType::B, PieceType::G, PieceType::S,
    PieceType::N, PieceType::L, PieceType::P, PieceType::G, PieceType::S,
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
            if !used.contains(&(x, y)) || guard >= 40 { break; }
        }
        if used.contains(&(x, y)) { continue; }
        used.insert((x, y));
        pieces.push(PieceData { x, y, owner, piece_type: t });
    }
}

/// 手数に応じたパラメータでランダム候補局面を生成する
fn random_candidate(rng: &mut Rng, params: &CandidateParams) -> Option<InitialData> {
    let mut pieces = Vec::new();
    let mut used = HashSet::new();

    let dk = PieceData { x: rng.ri(2, 8), y: rng.ri(1, 3), owner: Owner::Defender, piece_type: PieceType::K };
    used.insert((dk.x, dk.y));
    pieces.push(dk.clone());

    // 攻め方の駒を配置
    let atk_count = rng.ri(params.atk_count.0, params.atk_count.1) as usize;
    place_pieces_near_king(rng, &mut pieces, &mut used, &dk, Owner::Attacker,
        atk_count, &ATK_PIECE_TYPES, params.atk_range);

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
        },
        3 => CandidateParams {
            atk_count: (2, 3), atk_range: (-3, 3, -2, 4),
            def_count: (0, 2), def_range: (-2, 2, -1, 2),
            hand_prob: 0.3, major_hand_prob: 0.1,
        },
        _ => CandidateParams {
            atk_count: (2, 4), atk_range: (-4, 4, -2, 5),
            def_count: (1, 3), def_range: (-2, 2, -1, 3),
            hand_prob: 0.4, major_hand_prob: 0.15,
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

    atk_types.len() as i32 * 2 + drop_count * 5 + promote_count * 3 + unique_targets.len() as i32 * 2 - (piece_count - 8).max(0)
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

    hand_piece_count * 10
        + has_rook_in_hand * 8
        + has_bishop_in_hand * 6
        + drop_count * 5
        + promote_count * 3
        + defender_count * 2
        + attacker_count
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
            let state = cand.to_state();
            if validate_tsume_puzzle(&state, mate_length).is_some() {
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
    let normalized = strip_attacker_king(initial);
    let state = normalized.to_state();
    let solution = validate_tsume_puzzle(&state, mate_length)?;

    let pruned = prune_initial(&normalized, mate_length);
    let pruned_state = pruned.to_state();
    let (final_initial, final_solution) = if let Some(pruned_sol) = validate_tsume_puzzle(&pruned_state, mate_length) {
        (InitialData::from_state(&pruned_state), pruned_sol)
    } else {
        (InitialData::from_state(&state), solution)
    };

    // Remove unused attacker hand pieces; reject if stripping fails
    let (final_initial, final_solution) = strip_unused_hand(final_initial, final_solution, mate_length)?;

    // Normalize: mirror to right side if defender king is on the left half
    let (final_initial, final_solution) = normalize_right(final_initial, final_solution);

    let score = score_puzzle(&final_initial, &final_solution);
    Some((final_initial, final_solution, score))
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
    let new_state = initial.to_state();
    validate_tsume_puzzle(&new_state, mate_length)
        .map(|new_sol| (initial, new_sol))
}

/// Mirror the board so the defender king is on the right side (x >= 5),
/// which is the standard tsume-shogi convention.
fn normalize_right(initial: InitialData, solution: Vec<Move>) -> (InitialData, Vec<Move>) {
    let dk = initial.pieces.iter()
        .find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K);
    let needs_mirror = match dk {
        Some(k) => k.x < 5,
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

pub fn load_curated(path: &str) -> HashMap<u32, Vec<InitialData>> {
    let mut result = HashMap::new();
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

    let piece_count = init.pieces.len() as i32;
    let def_count = init.pieces.iter().filter(|p| p.owner == Owner::Defender && p.piece_type != PieceType::K).count() as i32;

    // King position region (3x3 grid)
    let dk_region_x = ((dk.0 - 1) / 3) as i32;
    let dk_region_y = ((dk.1 - 1) / 3) as i32;

    vec![
        has_r * 6, has_b * 6, has_g * 6, has_s * 6, has_n * 6, has_l * 6, has_p * 6,
        hand_r * 6, hand_b * 6, hand_g * 6, hand_s * 6, hand_n * 6, hand_l * 6, hand_p * 6,
        has_drop * 3, has_promote * 3,
        piece_count, def_count,
        dk_region_x * 5, dk_region_y * 5,
        dk.0 as i32, dk.1 as i32,
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
    max: usize,
) -> Vec<(InitialData, Vec<Move>, i32)> {
    if pool.is_empty() { return vec![]; }

    let count = max.min(pool.len());

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

    // Take only the number we need
    indices.truncate(count);

    // Split into 4 tiers (quartiles)
    let num_tiers = 4usize;
    let tier_size = indices.len().div_ceil(num_tiers);

    let mut result = Vec::with_capacity(count);
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

pub fn generate_puzzles(seed: u64, mate_length: u32, attempts: u32, curated_seeds: &[InitialData], max: u32) -> Vec<Puzzle> {
    let mut sig_set: HashSet<String> = HashSet::new();
    let mut struct_set: HashSet<String> = HashSet::new();
    let mut comp_count: HashMap<String, u32> = HashMap::new();
    let mut atk_comp_count: HashMap<String, u32> = HashMap::new();
    let max_per_composition: u32 = 3; // 同一駒構成のパズルは最大3問
    let max_per_atk_composition: u32 = 5; // 攻め方の駒構成が同じパズルは最大5問
    let mut results: Vec<(InitialData, Vec<Move>, i32)> = Vec::new();

    let add_result = |initial: InitialData, _solution: Vec<Move>, _score: i32,
                      sig_set: &mut HashSet<String>, struct_set: &mut HashSet<String>,
                      comp_count: &mut HashMap<String, u32>,
                      atk_comp_count: &mut HashMap<String, u32>| -> bool {
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

    eprintln!("  {}手詰: Starting parallel generation with {} attempts...", mate_length, attempts);

    // Generate candidates in parallel using rayon
    let batch_size = 1000u32;
    let num_batches = attempts.div_ceil(batch_size);

    for batch in 0..num_batches {
        if results.len() as u32 >= max { break; }

        let batch_start = batch * batch_size;
        let batch_end = (batch_start + batch_size).min(attempts);
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
                results.push((fin.clone(), sol, score));

                // Try mutations of found puzzles
                let mut rng = Rng::new(seed.wrapping_add(results.len() as u64 * 999983));
                for _ in 0..20 {
                    if results.len() as u32 >= max { break; }
                    if let Some(mutated) = mutate_initial(&mut rng, &fin) {
                        if let Some((mfin, msol, mscore)) = validate_and_prune(&mutated, mate_length) {
                            if add_result(mfin.clone(), msol.clone(), mscore, &mut sig_set, &mut struct_set, &mut comp_count, &mut atk_comp_count) {
                                results.push((mfin, msol, mscore));
                            }
                        }
                    }
                }

                // Try mirror
                let mirrored = fin.mirror();
                if basic_validity(&mirrored) {
                    if let Some((mfin, msol, mscore)) = validate_and_prune(&mirrored, mate_length) {
                        if add_result(mfin.clone(), msol.clone(), mscore, &mut sig_set, &mut struct_set, &mut comp_count, &mut atk_comp_count) {
                            results.push((mfin, msol, mscore));
                        }
                    }
                }
            }
        }

        if batch % 10 == 0 && batch > 0 {
            eprintln!("  {}手詰: {}/{} attempts, {} found", mate_length, batch_end, attempts, results.len());
        }
    }

    // Phase 2: mutate from found results
    if !results.is_empty() {
        let mutate_attempts = attempts / 2;
        let mut rng = Rng::new(seed.wrapping_add(77777));

        for i in 0..mutate_attempts {
            if results.len() as u32 >= max { break; }
            let seed_idx = rng.next() as usize % results.len();
            let seed_initial = results[seed_idx].0.clone();

            if let Some(cand) = mutate_initial(&mut rng, &seed_initial) {
                if let Some((fin, sol, score)) = validate_and_prune(&cand, mate_length) {
                    if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count, &mut atk_comp_count) {
                        results.push((fin, sol, score));
                    }
                }
            }

            if i % 10000 == 0 && i > 0 {
                eprintln!("  {}手詰: mutation phase {}/{}, {} found", mate_length, i, mutate_attempts, results.len());
            }
        }
    }

    // Diversify ordering: greedy "pick the most different puzzle next"
    let final_results = diversify_order(results, max as usize);

    final_results.iter().enumerate().map(|(i, (init, sol, score))| {
        Puzzle {
            id: i as u32 + 1,
            mate_length,
            initial: init.clone(),
            solution: sol.clone(),
            quality: "validated".to_string(),
            score: *score,
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
            assert_eq!(rng1.next(), rng2.next());
        }
    }

    #[test]
    fn test_rng_different_seeds() {
        let mut rng1 = Rng::new(1);
        let mut rng2 = Rng::new(2);
        assert_ne!(rng1.next(), rng2.next());
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
    fn test_diversify_order_respects_max() {
        let pool: Vec<(InitialData, Vec<Move>, i32)> = (0..10).map(|i| {
            let init = mk_initial(vec![
                PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
                PieceData { x: (i % 8 + 1) as i8, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
                PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::G },
            ]);
            let sol = vec![Move { from: Some([3, 5]), to: [3, 4], drop: None, promote: false }];
            (init, sol, i)
        }).collect();
        let result = diversify_order(pool, 5);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_diversify_order_empty() {
        let result = diversify_order(vec![], 10);
        assert!(result.is_empty());
    }
}
