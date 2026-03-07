use std::collections::{HashMap, HashSet};
use std::fs;
use rayon::prelude::*;
use serde::{Serialize, Deserialize};

use crate::shogi::*;

#[derive(Debug, Clone, Serialize)]
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

    let ak = initial.pieces.iter().find(|p| p.owner == Owner::Attacker && p.piece_type == PieceType::K);
    let dk = initial.pieces.iter().find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K);
    let (ak, dk) = match (ak, dk) {
        (Some(a), Some(d)) => (a, d),
        _ => return false,
    };

    if (ak.x - dk.x).abs() <= 1 && (ak.y - dk.y).abs() <= 1 { return false; }

    true
}

fn structural_signature(initial: &InitialData) -> String {
    let dk = match initial.pieces.iter().find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K) {
        Some(k) => k,
        None => return String::new(),
    };

    let mut rel_base: Vec<_> = initial.pieces.iter()
        .map(|p| (p.owner, p.piece_type, p.x - dk.x, p.y - dk.y))
        .collect();
    rel_base.sort_by(|a, b| {
        format!("{:?}", a.0).cmp(&format!("{:?}", b.0))
            .then(format!("{:?}", a.1).cmp(&format!("{:?}", b.1)))
            .then(a.3.cmp(&b.3))
            .then(a.2.cmp(&b.2))
    });

    let mut rel_mirror: Vec<_> = initial.pieces.iter()
        .map(|p| (p.owner, p.piece_type, -(p.x - dk.x), p.y - dk.y))
        .collect();
    rel_mirror.sort_by(|a, b| {
        format!("{:?}", a.0).cmp(&format!("{:?}", b.0))
            .then(format!("{:?}", a.1).cmp(&format!("{:?}", b.1)))
            .then(a.3.cmp(&b.3))
            .then(a.2.cmp(&b.2))
    });

    let a = format!("{:?}{:?}", rel_base, initial.hands);
    let b = format!("{:?}{:?}", rel_mirror, initial.hands);
    if a < b { a } else { b }
}

fn random_candidate_3(rng: &mut Rng) -> Option<InitialData> {
    let mut pieces = Vec::new();
    let mut used = HashSet::new();

    let dk = PieceData { x: rng.ri(2, 8), y: rng.ri(1, 3), owner: Owner::Defender, piece_type: PieceType::K };
    let ak = PieceData { x: rng.ri(1, 9), y: rng.ri(7, 9), owner: Owner::Attacker, piece_type: PieceType::K };
    if (dk.x - ak.x).abs() <= 1 && (dk.y - ak.y).abs() <= 1 { return None; }
    used.insert((dk.x, dk.y));
    used.insert((ak.x, ak.y));
    pieces.push(ak);
    pieces.push(dk.clone());

    let atk_count = rng.ri(2, 3) as usize;
    let atk_types = [PieceType::R, PieceType::B, PieceType::G, PieceType::S, PieceType::P, PieceType::G, PieceType::S];
    for _ in 0..atk_count {
        let t = *rng.pick(&atk_types);
        let mut x;
        let mut y;
        let mut g = 0;
        loop {
            x = (dk.x + rng.ri(-3, 3)).max(1).min(9);
            y = (dk.y + rng.ri(-2, 4)).max(1).min(9);
            g += 1;
            if !used.contains(&(x, y)) || g >= 40 { break; }
        }
        if used.contains(&(x, y)) { continue; }
        used.insert((x, y));
        pieces.push(PieceData { x, y, owner: Owner::Attacker, piece_type: t });
    }

    let def_count = rng.ri(0, 2) as usize;
    let def_types = [PieceType::G, PieceType::S, PieceType::P, PieceType::G, PieceType::S];
    for _ in 0..def_count {
        let t = *rng.pick(&def_types);
        let mut x;
        let mut y;
        let mut g = 0;
        loop {
            x = (dk.x + rng.ri(-2, 2)).max(1).min(9);
            y = (dk.y + rng.ri(-1, 2)).max(1).min(9);
            g += 1;
            if !used.contains(&(x, y)) || g >= 40 { break; }
        }
        if used.contains(&(x, y)) { continue; }
        used.insert((x, y));
        pieces.push(PieceData { x, y, owner: Owner::Defender, piece_type: t });
    }

    let mut hands = empty_hands_data();
    if rng.next_f64() < 0.3 {
        let hand_types = [PieceType::P, PieceType::S, PieceType::G];
        match rng.pick(&hand_types) {
            PieceType::P => hands.attacker.P = 1,
            PieceType::S => hands.attacker.S = 1,
            PieceType::G => hands.attacker.G = 1,
            _ => {}
        }
    }
    if rng.next_f64() < 0.1 {
        let hand_types = [PieceType::R, PieceType::B];
        match rng.pick(&hand_types) {
            PieceType::R => hands.attacker.R = 1,
            PieceType::B => hands.attacker.B = 1,
            _ => {}
        }
    }

    Some(InitialData { pieces, hands, side_to_move: Owner::Attacker })
}

fn random_candidate_5(rng: &mut Rng) -> Option<InitialData> {
    let mut pieces = Vec::new();
    let mut used = HashSet::new();

    let dk = PieceData { x: rng.ri(2, 8), y: rng.ri(1, 3), owner: Owner::Defender, piece_type: PieceType::K };
    let ak = PieceData { x: rng.ri(1, 9), y: rng.ri(7, 9), owner: Owner::Attacker, piece_type: PieceType::K };
    if (dk.x - ak.x).abs() <= 1 && (dk.y - ak.y).abs() <= 1 { return None; }
    used.insert((dk.x, dk.y));
    used.insert((ak.x, ak.y));
    pieces.push(ak);
    pieces.push(dk.clone());

    let atk_count = rng.ri(2, 4) as usize;
    let atk_types = [PieceType::R, PieceType::B, PieceType::G, PieceType::S, PieceType::P, PieceType::R, PieceType::G, PieceType::S];
    for _ in 0..atk_count {
        let t = *rng.pick(&atk_types);
        let mut x;
        let mut y;
        let mut g = 0;
        loop {
            x = (dk.x + rng.ri(-4, 4)).max(1).min(9);
            y = (dk.y + rng.ri(-2, 5)).max(1).min(9);
            g += 1;
            if !used.contains(&(x, y)) || g >= 40 { break; }
        }
        if used.contains(&(x, y)) { continue; }
        used.insert((x, y));
        pieces.push(PieceData { x, y, owner: Owner::Attacker, piece_type: t });
    }

    let def_count = rng.ri(1, 3) as usize;
    let def_types = [PieceType::G, PieceType::S, PieceType::P, PieceType::G, PieceType::S, PieceType::R];
    for _ in 0..def_count {
        let t = *rng.pick(&def_types);
        let mut x;
        let mut y;
        let mut g = 0;
        loop {
            x = (dk.x + rng.ri(-2, 2)).max(1).min(9);
            y = (dk.y + rng.ri(-1, 3)).max(1).min(9);
            g += 1;
            if !used.contains(&(x, y)) || g >= 40 { break; }
        }
        if used.contains(&(x, y)) { continue; }
        used.insert((x, y));
        pieces.push(PieceData { x, y, owner: Owner::Defender, piece_type: t });
    }

    let mut hands = empty_hands_data();
    if rng.next_f64() < 0.4 {
        let hand_types = [PieceType::P, PieceType::S, PieceType::G];
        match rng.pick(&hand_types) {
            PieceType::P => hands.attacker.P = 1,
            PieceType::S => hands.attacker.S = 1,
            PieceType::G => hands.attacker.G = 1,
            _ => {}
        }
    }
    if rng.next_f64() < 0.15 {
        let hand_types = [PieceType::R, PieceType::B];
        match rng.pick(&hand_types) {
            PieceType::R => hands.attacker.R = 1,
            PieceType::B => hands.attacker.B = 1,
            _ => {}
        }
    }

    Some(InitialData { pieces, hands, side_to_move: Owner::Attacker })
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
            let types = if cand.pieces[idx].owner == Owner::Attacker {
                vec![PieceType::R, PieceType::B, PieceType::G, PieceType::S, PieceType::P]
            } else {
                vec![PieceType::G, PieceType::S, PieceType::P]
            };
            cand.pieces[idx].piece_type = *rng.pick(&types);
        }
        "add-piece" => {
            let owners = [Owner::Attacker, Owner::Defender];
            let owner = *rng.pick(&owners);
            let types = if owner == Owner::Attacker {
                vec![PieceType::R, PieceType::G, PieceType::S, PieceType::P, PieceType::B]
            } else {
                vec![PieceType::G, PieceType::S, PieceType::P]
            };
            let t = *rng.pick(&types);
            if let Some(dk) = cand.pieces.iter().find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K) {
                let x = (dk.x + rng.ri(-3, 3)).max(1).min(9);
                let y = (dk.y + rng.ri(-2, 4)).max(1).min(9);
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
            let types = [PieceType::P, PieceType::S, PieceType::G, PieceType::R, PieceType::B];
            let t = *rng.pick(&types);
            let v = rng.ri(0, 1) as u8;
            match t {
                PieceType::P => cand.hands.attacker.P = v,
                PieceType::S => cand.hands.attacker.S = v,
                PieceType::G => cand.hands.attacker.G = v,
                PieceType::R => cand.hands.attacker.R = v,
                PieceType::B => cand.hands.attacker.B = v,
                _ => {}
            }
        }
        _ => {}
    }

    // Clamp coordinates
    for p in &mut cand.pieces {
        p.x = p.x.max(1).min(9);
        p.y = p.y.max(1).min(9);
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
    let hand_piece_count = (h.R + h.B + h.G + h.S + h.P) as i32;
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
        + attacker_count * 1
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
                .then((b.1.y as i8 - 5).abs().cmp(&(a.1.y as i8 - 5).abs()))
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

fn validate_and_prune(initial: &InitialData, mate_length: u32) -> Option<(InitialData, Vec<Move>, i32)> {
    let state = initial.to_state();
    let solution = validate_tsume_puzzle(&state, mate_length)?;

    let pruned = prune_initial(initial, mate_length);
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

/// Replay the solution, remove unused attacker hand pieces, and re-validate.
/// Returns None if the puzzle can't be fixed (unused hand pieces that can't be removed).
fn strip_unused_hand(mut initial: InitialData, solution: Vec<Move>, mate_length: u32) -> Option<(InitialData, Vec<Move>)> {
    let mut final_state = initial.to_state();
    for m in &solution {
        final_state = apply_move(&final_state, m);
    }
    let remaining = &final_state.hands.attacker;
    if remaining.iter().sum::<u8>() == 0 {
        return Some((initial, solution)); // nothing to strip
    }
    // Strip unused hand pieces
    initial.hands.attacker.R = initial.hands.attacker.R.saturating_sub(remaining[0]);
    initial.hands.attacker.B = initial.hands.attacker.B.saturating_sub(remaining[1]);
    initial.hands.attacker.G = initial.hands.attacker.G.saturating_sub(remaining[2]);
    initial.hands.attacker.S = initial.hands.attacker.S.saturating_sub(remaining[3]);
    initial.hands.attacker.P = initial.hands.attacker.P.saturating_sub(remaining[4]);
    // Re-validate: stripping hand pieces might change the solution
    let new_state = initial.to_state();
    if let Some(new_sol) = validate_tsume_puzzle(&new_state, mate_length) {
        Some((initial, new_sol))
    } else {
        None // reject: can't strip unused hand pieces
    }
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
    let has_p = atk_pieces.iter().any(|p| p.piece_type == PieceType::P) as i32;

    let h = &init.hands.attacker;
    let hand_r = (h.R > 0) as i32;
    let hand_b = (h.B > 0) as i32;
    let hand_g = (h.G > 0) as i32;
    let hand_s = (h.S > 0) as i32;
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
        has_r * 3, has_b * 3, has_g * 3, has_s * 3, has_p * 3,  // board piece types
        hand_r * 4, hand_b * 4, hand_g * 4, hand_s * 4, hand_p * 4,  // hand pieces
        has_drop * 3, has_promote * 3,
        piece_count, def_count,
        dk_region_x * 5, dk_region_y * 5,  // king position (high weight to avoid repeats)
        dk.0 as i32, dk.1 as i32,          // exact king position for fine-grained distance
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
    let tier_size = (indices.len() + num_tiers - 1) / num_tiers;

    let mut result = Vec::with_capacity(count);
    for tier in indices.chunks(tier_size) {
        let diversified = diversify_within(tier, &features);
        for &idx in &diversified {
            result.push(pool[idx].clone());
        }
    }
    result
}

/// Key representing the piece composition (types + hands, ignoring positions)
fn composition_key(initial: &InitialData) -> String {
    let mut atk_types: Vec<&str> = initial.pieces.iter()
        .filter(|p| p.owner == Owner::Attacker && p.piece_type != PieceType::K)
        .map(|p| match p.piece_type {
            PieceType::R => "R", PieceType::B => "B", PieceType::G => "G",
            PieceType::S => "S", PieceType::P => "P", _ => "?",
        })
        .collect();
    atk_types.sort();
    let mut def_types: Vec<&str> = initial.pieces.iter()
        .filter(|p| p.owner == Owner::Defender && p.piece_type != PieceType::K)
        .map(|p| match p.piece_type {
            PieceType::R => "R", PieceType::B => "B", PieceType::G => "G",
            PieceType::S => "S", PieceType::P => "P", _ => "?",
        })
        .collect();
    def_types.sort();
    let h = &initial.hands.attacker;
    format!("a:{} d:{} h:R{}B{}G{}S{}P{}",
        atk_types.join(""), def_types.join(""),
        h.R, h.B, h.G, h.S, h.P)
}

pub fn generate_puzzles(seed: u64, mate_length: u32, attempts: u32, curated_seeds: &[InitialData], max: u32) -> Vec<Puzzle> {
    let mut sig_set: HashSet<String> = HashSet::new();
    let mut struct_set: HashSet<String> = HashSet::new();
    let mut comp_count: HashMap<String, u32> = HashMap::new();
    let max_per_composition: u32 = 3; // At most 3 puzzles with identical piece composition
    let mut results: Vec<(InitialData, Vec<Move>, i32)> = Vec::new();

    let add_result = |initial: InitialData, _solution: Vec<Move>, _score: i32,
                      sig_set: &mut HashSet<String>, struct_set: &mut HashSet<String>,
                      comp_count: &mut HashMap<String, u32>| -> bool {
        let sig = serde_json::to_string(&initial).unwrap_or_default();
        let ssig = structural_signature(&initial);
        if sig_set.contains(&sig) || struct_set.contains(&ssig) { return false; }
        let ckey = composition_key(&initial);
        let count = comp_count.get(&ckey).copied().unwrap_or(0);
        if count >= max_per_composition { return false; }
        sig_set.insert(sig);
        struct_set.insert(ssig);
        *comp_count.entry(ckey).or_insert(0) += 1;
        true
    };

    // Add curated puzzles
    for initial in curated_seeds {
        if let Some((fin, sol, score)) = validate_and_prune(initial, mate_length) {
            if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count) {
                results.push((fin, sol, score));
            }
        }
    }

    // Mirror curated
    for initial in curated_seeds {
        let mirrored = initial.mirror();
        if basic_validity(&mirrored) {
            if let Some((fin, sol, score)) = validate_and_prune(&mirrored, mate_length) {
                if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count) {
                    results.push((fin, sol, score));
                }
            }
        }
    }

    eprintln!("  {}手詰: Starting parallel generation with {} attempts...", mate_length, attempts);

    // Generate candidates in parallel using rayon
    let batch_size = 1000u32;
    let num_batches = (attempts + batch_size - 1) / batch_size;

    for batch in 0..num_batches {
        if results.len() as u32 >= max { break; }

        let batch_start = batch * batch_size;
        let batch_end = (batch_start + batch_size).min(attempts);
        let batch_range: Vec<u32> = (batch_start..batch_end).collect();

        // Generate and validate candidates in parallel
        let found: Vec<(InitialData, Vec<Move>, i32)> = batch_range.par_iter()
            .filter_map(|&i| {
                let mut rng = Rng::new(seed.wrapping_add(i as u64).wrapping_mul(2654435761));
                let cand = if mate_length == 3 {
                    random_candidate_3(&mut rng)
                } else {
                    random_candidate_5(&mut rng)
                };
                let cand = cand?;
                validate_and_prune(&cand, mate_length)
            })
            .collect();

        for (fin, sol, score) in found {
            if results.len() as u32 >= max { break; }
            if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count) {
                results.push((fin.clone(), sol, score));

                // Try mutations of found puzzles
                let mut rng = Rng::new(seed.wrapping_add(results.len() as u64 * 999983));
                for _ in 0..20 {
                    if results.len() as u32 >= max { break; }
                    if let Some(mutated) = mutate_initial(&mut rng, &fin) {
                        if let Some((mfin, msol, mscore)) = validate_and_prune(&mutated, mate_length) {
                            if add_result(mfin.clone(), msol.clone(), mscore, &mut sig_set, &mut struct_set, &mut comp_count) {
                                results.push((mfin, msol, mscore));
                            }
                        }
                    }
                }

                // Try mirror
                let mirrored = fin.mirror();
                if basic_validity(&mirrored) {
                    if let Some((mfin, msol, mscore)) = validate_and_prune(&mirrored, mate_length) {
                        if add_result(mfin.clone(), msol.clone(), mscore, &mut sig_set, &mut struct_set, &mut comp_count) {
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
                    if add_result(fin.clone(), sol.clone(), score, &mut sig_set, &mut struct_set, &mut comp_count) {
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
