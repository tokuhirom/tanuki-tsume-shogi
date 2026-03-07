use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
pub const BOARD_SIZE: i8 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Owner {
    #[serde(rename = "attacker")]
    Attacker,
    #[serde(rename = "defender")]
    Defender,
}

impl Owner {
    pub fn opposite(self) -> Owner {
        match self {
            Owner::Attacker => Owner::Defender,
            Owner::Defender => Owner::Attacker,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PieceType {
    K, R, B, G, S, N, L, P,
    #[serde(rename = "+R")]
    PR,
    #[serde(rename = "+B")]
    PB,
    #[serde(rename = "+S")]
    PS,
    #[serde(rename = "+N")]
    PN,
    #[serde(rename = "+L")]
    PL,
    #[serde(rename = "+P")]
    PP,
}

impl PieceType {
    pub fn is_promotable(self) -> bool {
        matches!(self, PieceType::R | PieceType::B | PieceType::S | PieceType::N | PieceType::L | PieceType::P)
    }

    pub fn promote(self) -> PieceType {
        match self {
            PieceType::R => PieceType::PR,
            PieceType::B => PieceType::PB,
            PieceType::S => PieceType::PS,
            PieceType::N => PieceType::PN,
            PieceType::L => PieceType::PL,
            PieceType::P => PieceType::PP,
            other => other,
        }
    }

    pub fn unpromote(self) -> PieceType {
        match self {
            PieceType::PR => PieceType::R,
            PieceType::PB => PieceType::B,
            PieceType::PS => PieceType::S,
            PieceType::PN => PieceType::N,
            PieceType::PL => PieceType::L,
            PieceType::PP => PieceType::P,
            other => other,
        }
    }

    pub fn is_promoted(self) -> bool {
        matches!(self, PieceType::PR | PieceType::PB | PieceType::PS | PieceType::PN | PieceType::PL | PieceType::PP)
    }

    #[allow(dead_code)]
    pub fn is_hand_type(self) -> bool {
        matches!(self, PieceType::R | PieceType::B | PieceType::G | PieceType::S | PieceType::N | PieceType::L | PieceType::P)
    }
}

pub const HAND_TYPES: [PieceType; 7] = [PieceType::R, PieceType::B, PieceType::G, PieceType::S, PieceType::N, PieceType::L, PieceType::P];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pos {
    pub x: i8,
    pub y: i8,
}

impl Pos {
    pub fn new(x: i8, y: i8) -> Self { Pos { x, y } }
    pub fn is_valid(self) -> bool { self.x >= 1 && self.x <= 9 && self.y >= 1 && self.y <= 9 }
    pub fn idx(self) -> usize { ((self.y - 1) * 9 + (self.x - 1)) as usize }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoardPiece {
    pub owner: Owner,
    pub piece_type: PieceType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hands {
    pub attacker: [u8; 7], // R, B, G, S, N, L, P
    pub defender: [u8; 7],
}

impl Hands {
    pub fn empty() -> Self {
        Hands { attacker: [0; 7], defender: [0; 7] }
    }

    fn hand_idx(t: PieceType) -> usize {
        match t {
            PieceType::R => 0,
            PieceType::B => 1,
            PieceType::G => 2,
            PieceType::S => 3,
            PieceType::N => 4,
            PieceType::L => 5,
            PieceType::P => 6,
            _ => panic!("not a hand type"),
        }
    }

    pub fn get(&self, owner: Owner, t: PieceType) -> u8 {
        let idx = Self::hand_idx(t);
        match owner {
            Owner::Attacker => self.attacker[idx],
            Owner::Defender => self.defender[idx],
        }
    }

    pub fn set(&mut self, owner: Owner, t: PieceType, val: u8) {
        let idx = Self::hand_idx(t);
        match owner {
            Owner::Attacker => self.attacker[idx] = val,
            Owner::Defender => self.defender[idx] = val,
        }
    }

    pub fn add(&mut self, owner: Owner, t: PieceType, n: u8) {
        let v = self.get(owner, t);
        self.set(owner, t, v + n);
    }

    pub fn sub(&mut self, owner: Owner, t: PieceType, n: u8) {
        let v = self.get(owner, t);
        self.set(owner, t, v.saturating_sub(n));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct State {
    pub board: [Option<BoardPiece>; 81],
    pub hands: Hands,
    pub side_to_move: Owner,
}

impl State {
    pub fn new() -> Self {
        State {
            board: [None; 81],
            hands: Hands::empty(),
            side_to_move: Owner::Attacker,
        }
    }

    pub fn get(&self, pos: Pos) -> Option<BoardPiece> {
        self.board[pos.idx()]
    }

    pub fn set(&mut self, pos: Pos, piece: Option<BoardPiece>) {
        self.board[pos.idx()] = piece;
    }

    pub fn king_pos(&self, owner: Owner) -> Option<Pos> {
        for y in 1..=9i8 {
            for x in 1..=9i8 {
                let p = Pos::new(x, y);
                if let Some(bp) = self.get(p) {
                    if bp.owner == owner && bp.piece_type == PieceType::K {
                        return Some(p);
                    }
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Move {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<[i8; 2]>,
    pub to: [i8; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drop: Option<PieceType>,
    pub promote: bool,
}

impl Move {
    pub fn to_string_key(&self) -> String {
        if let Some(d) = self.drop {
            format!("{:?}*{}{}", d, self.to[0], self.to[1])
        } else if let Some(f) = self.from {
            format!("{}{}-{}{}{}", f[0], f[1], self.to[0], self.to[1], if self.promote { "+" } else { "" })
        } else {
            String::new()
        }
    }
}

fn promotion_zone(owner: Owner, y: i8) -> bool {
    match owner {
        Owner::Attacker => y <= 3,
        Owner::Defender => y >= 7,
    }
}

fn transform_dir(owner: Owner, dx: i8, dy: i8) -> (i8, i8) {
    match owner {
        Owner::Attacker => (dx, dy),
        Owner::Defender => (-dx, -dy),
    }
}

fn step_moves(t: PieceType) -> &'static [(i8, i8)] {
    match t {
        PieceType::K => &[(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)],
        PieceType::G | PieceType::PP | PieceType::PS | PieceType::PN | PieceType::PL => &[(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(0,1)],
        PieceType::S => &[(-1,-1),(0,-1),(1,-1),(-1,1),(1,1)],
        PieceType::N => &[(-1,-2),(1,-2)],
        PieceType::P => &[(0,-1)],
        _ => &[],
    }
}

fn slide_dirs(t: PieceType) -> &'static [(i8, i8)] {
    match t {
        PieceType::R | PieceType::PR => &[(0,-1),(1,0),(0,1),(-1,0)],
        PieceType::B | PieceType::PB => &[(1,-1),(1,1),(-1,1),(-1,-1)],
        PieceType::L => &[(0,-1)],
        _ => &[],
    }
}

fn extra_steps(t: PieceType) -> &'static [(i8, i8)] {
    match t {
        PieceType::PR => &[(-1,-1),(1,-1),(1,1),(-1,1)],
        PieceType::PB => &[(0,-1),(1,0),(0,1),(-1,0)],
        _ => &[],
    }
}

fn is_move_promotion_legal(owner: Owner, t: PieceType, from_y: i8, to_y: i8, promote: bool) -> bool {
    if !t.is_promotable() { return !promote; }
    let can_promote = promotion_zone(owner, from_y) || promotion_zone(owner, to_y);
    if !can_promote { return !promote; }
    if !promote {
        // 行き場のない駒の禁止: 歩・香は最奥段、桂は奥2段に不成で行けない
        match t {
            PieceType::P | PieceType::L => {
                if owner == Owner::Attacker && to_y == 1 { return false; }
                if owner == Owner::Defender && to_y == 9 { return false; }
            }
            PieceType::N => {
                if owner == Owner::Attacker && to_y <= 2 { return false; }
                if owner == Owner::Defender && to_y >= 8 { return false; }
            }
            _ => {}
        }
    }
    true
}

fn has_pawn_on_file(state: &State, owner: Owner, x: i8) -> bool {
    for y in 1..=9i8 {
        if let Some(bp) = state.get(Pos::new(x, y)) {
            if bp.owner == owner && bp.piece_type == PieceType::P { return true; }
        }
    }
    false
}

fn add_moves_for_target(moves: &mut Vec<Move>, owner: Owner, piece_type: PieceType, fx: i8, fy: i8, nx: i8, ny: i8) {
    if piece_type.is_promoted() {
        moves.push(Move { from: Some([fx, fy]), to: [nx, ny], drop: None, promote: false });
    } else if piece_type.is_promotable() && (promotion_zone(owner, fy) || promotion_zone(owner, ny)) {
        if is_move_promotion_legal(owner, piece_type, fy, ny, false) {
            moves.push(Move { from: Some([fx, fy]), to: [nx, ny], drop: None, promote: false });
        }
        if is_move_promotion_legal(owner, piece_type, fy, ny, true) {
            moves.push(Move { from: Some([fx, fy]), to: [nx, ny], drop: None, promote: true });
        }
    } else {
        moves.push(Move { from: Some([fx, fy]), to: [nx, ny], drop: None, promote: false });
    }
}

fn pseudo_moves_from(state: &State, pos: Pos, piece: BoardPiece) -> Vec<Move> {
    let mut moves = Vec::new();
    let owner = piece.owner;
    let t = piece.piece_type;

    for &(dx, dy) in step_moves(t) {
        let (tx, ty) = transform_dir(owner, dx, dy);
        let np = Pos::new(pos.x + tx, pos.y + ty);
        if !np.is_valid() { continue; }
        if let Some(target) = state.get(np) {
            if target.owner == owner { continue; }
        }
        add_moves_for_target(&mut moves, owner, t, pos.x, pos.y, np.x, np.y);
    }

    for &(dx, dy) in slide_dirs(t) {
        let (tx, ty) = transform_dir(owner, dx, dy);
        let mut nx = pos.x + tx;
        let mut ny = pos.y + ty;
        while Pos::new(nx, ny).is_valid() {
            let target = state.get(Pos::new(nx, ny));
            if let Some(t_piece) = target {
                if t_piece.owner == owner { break; }
            }
            add_moves_for_target(&mut moves, owner, t, pos.x, pos.y, nx, ny);
            if target.is_some() { break; }
            nx += tx;
            ny += ty;
        }
    }

    for &(dx, dy) in extra_steps(t) {
        let (tx, ty) = transform_dir(owner, dx, dy);
        let np = Pos::new(pos.x + tx, pos.y + ty);
        if !np.is_valid() { continue; }
        if let Some(target) = state.get(np) {
            if target.owner == owner { continue; }
        }
        moves.push(Move { from: Some([pos.x, pos.y]), to: [np.x, np.y], drop: None, promote: false });
    }

    moves
}

fn pseudo_drops(state: &State, owner: Owner) -> Vec<Move> {
    let mut out = Vec::new();
    for &t in &HAND_TYPES {
        if state.hands.get(owner, t) == 0 { continue; }
        for y in 1..=9i8 {
            for x in 1..=9i8 {
                let p = Pos::new(x, y);
                if state.get(p).is_some() { continue; }
                // 行き場のない場所への打ち駒禁止
                match t {
                    PieceType::P | PieceType::L => {
                        if owner == Owner::Attacker && y == 1 { continue; }
                        if owner == Owner::Defender && y == 9 { continue; }
                    }
                    PieceType::N => {
                        if owner == Owner::Attacker && y <= 2 { continue; }
                        if owner == Owner::Defender && y >= 8 { continue; }
                    }
                    _ => {}
                }
                if t == PieceType::P
                    && has_pawn_on_file(state, owner, x) { continue; }
                out.push(Move { from: None, to: [x, y], drop: Some(t), promote: false });
            }
        }
    }
    out
}

/// 指し手を適用して新しい局面を返す（元の局面は変更しない）
pub fn apply_move(state: &State, m: &Move) -> State {
    let mut next = state.clone();
    let owner = state.side_to_move;

    if let Some(drop_type) = m.drop {
        next.hands.sub(owner, drop_type, 1);
        let tp = Pos::new(m.to[0], m.to[1]);
        next.set(tp, Some(BoardPiece { owner, piece_type: drop_type }));
    } else {
        let from = m.from.unwrap();
        let fp = Pos::new(from[0], from[1]);
        let src = state.get(fp).unwrap();
        let tp = Pos::new(m.to[0], m.to[1]);

        if let Some(captured) = state.get(tp) {
            let cap_type = captured.piece_type.unpromote();
            if cap_type != PieceType::K {
                next.hands.add(owner, cap_type, 1);
            }
        }

        next.set(fp, None);
        let moved_type = if m.promote { src.piece_type.promote() } else { src.piece_type };
        next.set(tp, Some(BoardPiece { owner, piece_type: moved_type }));
    }

    next.side_to_move = owner.opposite();
    next
}

/// Check if a specific square is attacked by any piece of the given attacker
fn is_square_attacked(state: &State, target: Pos, attacker: Owner) -> bool {
    for y in 1..=9i8 {
        for x in 1..=9i8 {
            let p = Pos::new(x, y);
            if let Some(bp) = state.get(p) {
                if bp.owner != attacker { continue; }
                if can_reach(state, p, bp, target) {
                    return true;
                }
            }
        }
    }
    false
}

/// Fast check: can this piece reach the target square? (without generating all moves)
fn can_reach(state: &State, pos: Pos, piece: BoardPiece, target: Pos) -> bool {
    let owner = piece.owner;
    let t = piece.piece_type;

    for &(dx, dy) in step_moves(t) {
        let (tx, ty) = transform_dir(owner, dx, dy);
        if pos.x + tx == target.x && pos.y + ty == target.y {
            return true;
        }
    }

    for &(dx, dy) in slide_dirs(t) {
        let (tx, ty) = transform_dir(owner, dx, dy);
        let mut nx = pos.x + tx;
        let mut ny = pos.y + ty;
        while Pos::new(nx, ny).is_valid() {
            if nx == target.x && ny == target.y { return true; }
            if state.get(Pos::new(nx, ny)).is_some() { break; }
            nx += tx;
            ny += ty;
        }
    }

    for &(dx, dy) in extra_steps(t) {
        let (tx, ty) = transform_dir(owner, dx, dy);
        if pos.x + tx == target.x && pos.y + ty == target.y {
            return true;
        }
    }

    false
}

/// owner 側の玉に王手がかかっているか判定する
pub fn is_in_check(state: &State, owner: Owner) -> bool {
    let kp = match state.king_pos(owner) {
        Some(p) => p,
        None => return true,
    };
    is_square_attacked(state, kp, owner.opposite())
}

/// 手番側に合法手が1つでもあるか判定する（全手生成より高速）
fn has_any_legal_move(state: &State) -> bool {
    let owner = state.side_to_move;

    for y in 1..=9i8 {
        for x in 1..=9i8 {
            let p = Pos::new(x, y);
            if let Some(bp) = state.get(p) {
                if bp.owner != owner { continue; }
                for m in pseudo_moves_from(state, p, bp) {
                    let next = apply_move(state, &m);
                    if !is_in_check(&next, owner) {
                        return true;
                    }
                }
            }
        }
    }

    for m in pseudo_drops(state, owner) {
        let next = apply_move(state, &m);
        if is_in_check(&next, owner) { continue; }
        // 打ち歩詰めチェック
        if m.drop == Some(PieceType::P) {
            let enemy = owner.opposite();
            if is_in_check(&next, enemy) && !has_any_legal_move(&next) {
                continue;
            }
        }
        return true;
    }

    false
}

/// 打ち歩詰めの禁手チェック: 歩を打って王手かつ相手の合法手がない場合は禁止
fn pawn_drop_mate_forbidden(state: &State, m: &Move) -> bool {
    if m.drop != Some(PieceType::P) { return false; }
    let owner = state.side_to_move;
    let next = apply_move(state, m);
    let enemy = owner.opposite();
    if !is_in_check(&next, enemy) { return false; }
    !has_any_legal_move(&next)
}

/// 現在の手番側の合法手をすべて生成する
/// 自玉を王手に晒す手と打ち歩詰めは除外される
pub fn legal_moves(state: &State) -> Vec<Move> {
    let owner = state.side_to_move;
    let mut out = Vec::new();

    for y in 1..=9i8 {
        for x in 1..=9i8 {
            let p = Pos::new(x, y);
            if let Some(bp) = state.get(p) {
                if bp.owner != owner { continue; }
                for m in pseudo_moves_from(state, p, bp) {
                    let next = apply_move(state, &m);
                    if !is_in_check(&next, owner) {
                        out.push(m);
                    }
                }
            }
        }
    }

    for m in pseudo_drops(state, owner) {
        if pawn_drop_mate_forbidden(state, &m) { continue; }
        let next = apply_move(state, &m);
        if !is_in_check(&next, owner) {
            out.push(m);
        }
    }

    out
}

#[derive(Debug, Clone)]
pub struct MateResult {
    pub mate: bool,
    pub unique: bool,
    pub line: Vec<Move>,
}

fn state_key(state: &State, plies: u32) -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    state.hash(&mut hasher);
    plies.hash(&mut hasher);
    hasher.finish()
}

/// plies 手以内の詰みを探索する
/// 攻め方は王手のみ許可（詰将棋ルール）、守り方は最善応手を選ぶ
/// メモ化により同一局面の再計算を回避する
pub fn forced_mate_within(state: &State, plies: u32, memo: &mut HashMap<u64, MateResult>) -> MateResult {
    let key = state_key(state, plies);
    if let Some(cached) = memo.get(&key) {
        return cached.clone();
    }

    let side = state.side_to_move;
    let enemy = side.opposite();
    let moves = legal_moves(state);

    let result = if side == Owner::Defender {
        if moves.is_empty() {
            MateResult { mate: is_in_check(state, side), unique: true, line: vec![] }
        } else if plies == 0 {
            MateResult { mate: false, unique: false, line: vec![] }
        } else {
            // Early cutoff: if ANY defender move escapes mate, return false immediately
            let mut all_mate = true;
            let mut all_unique = true;
            let mut best_move: Option<Move> = None;
            let mut best_line: Vec<Move> = vec![];

            for m in &moves {
                let next = apply_move(state, m);
                let r = forced_mate_within(&next, plies - 1, memo);
                if !r.mate {
                    all_mate = false;
                    break; // Early cutoff!
                }
                if !r.unique { all_unique = false; }
                if best_move.is_none() {
                    best_move = Some(m.clone());
                    best_line = r.line;
                }
            }

            if let Some(bm) = best_move.filter(|_| all_mate) {
                let mut l = vec![bm];
                l.extend(best_line);
                MateResult { mate: true, unique: all_unique, line: l }
            } else {
                MateResult { mate: false, unique: false, line: vec![] }
            }
        }
    } else if plies == 0 {
        MateResult { mate: false, unique: false, line: vec![] }
    } else {
        // Only consider checking moves (tsume-shogi rule)
        let checks: Vec<_> = moves.iter()
            .filter(|m| {
                let n = apply_move(state, m);
                is_in_check(&n, enemy)
            })
            .collect();

        let mut winning: Vec<_> = checks.iter()
            .map(|m| {
                let next = apply_move(state, m);
                let r = forced_mate_within(&next, plies - 1, memo);
                ((*m).clone(), r)
            })
            .filter(|(_, r)| r.mate)
            .collect();

        if winning.is_empty() {
            MateResult { mate: false, unique: false, line: vec![] }
        } else {
            winning.sort_by(|a, b| a.0.to_string_key().cmp(&b.0.to_string_key()));
            let unique = winning.len() == 1 && winning[0].1.unique;
            let best = &winning[0];
            MateResult {
                mate: true,
                unique,
                line: {
                    let mut l = vec![best.0.clone()];
                    l.extend(best.1.line.iter().cloned());
                    l
                },
            }
        }
    };

    memo.insert(key, result.clone());
    result
}

/// 詰将棋の問題として有効か検証する
/// 条件: 奇数手、攻め方手番、両玉あり、初期王手なし、
///       指定手数で詰み、より短い手数では詰まない、解が唯一
/// 有効なら解の手順を返す
pub fn validate_tsume_puzzle(state: &State, mate_length: u32) -> Option<Vec<Move>> {
    if mate_length.is_multiple_of(2) || mate_length == 0 { return None; }
    if state.side_to_move != Owner::Attacker { return None; }
    if state.king_pos(Owner::Attacker).is_none() || state.king_pos(Owner::Defender).is_none() {
        return None;
    }

    // 初期局面で既に王手がかかっていてはならない
    if is_in_check(state, Owner::Defender) { return None; }

    let mut memo = HashMap::new();
    let within = forced_mate_within(state, mate_length, &mut memo);
    if !within.mate { return None; }

    if mate_length >= 3 {
        let shorter = forced_mate_within(state, mate_length - 2, &mut memo);
        if shorter.mate { return None; }
    }

    if !within.unique { return None; }

    Some(within.line)
}

// Conversion from/to JSON-serializable format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieceData {
    pub x: i8,
    pub y: i8,
    pub owner: Owner,
    #[serde(rename = "type")]
    pub piece_type: PieceType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandsData {
    pub attacker: HandCount,
    pub defender: HandCount,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct HandCount {
    #[serde(default)]
    pub R: u8,
    #[serde(default)]
    pub B: u8,
    #[serde(default)]
    pub G: u8,
    #[serde(default)]
    pub S: u8,
    #[serde(default)]
    pub N: u8,
    #[serde(default)]
    pub L: u8,
    #[serde(default)]
    pub P: u8,
}


impl HandCount {
    /// HAND_TYPES の順序 [R, B, G, S, N, L, P] に対応する配列に変換
    pub fn to_array(&self) -> [u8; 7] {
        [self.R, self.B, self.G, self.S, self.N, self.L, self.P]
    }

    /// HAND_TYPES の順序 [R, B, G, S, N, L, P] の配列から復元
    pub fn from_array(a: &[u8; 7]) -> Self {
        HandCount { R: a[0], B: a[1], G: a[2], S: a[3], N: a[4], L: a[5], P: a[6] }
    }

    /// 全駒種の合計枚数
    pub fn total(&self) -> u8 {
        self.R + self.B + self.G + self.S + self.N + self.L + self.P
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialData {
    pub pieces: Vec<PieceData>,
    pub hands: HandsData,
    #[serde(rename = "sideToMove")]
    pub side_to_move: Owner,
}

impl InitialData {
    pub fn to_state(&self) -> State {
        let mut state = State::new();
        for p in &self.pieces {
            let pos = Pos::new(p.x, p.y);
            state.set(pos, Some(BoardPiece { owner: p.owner, piece_type: p.piece_type }));
        }
        state.hands.attacker = self.hands.attacker.to_array();
        state.hands.defender = self.hands.defender.to_array();
        state.side_to_move = self.side_to_move;
        state
    }

    pub fn from_state(state: &State) -> Self {
        let mut pieces = Vec::new();
        for y in 1..=9i8 {
            for x in 1..=9i8 {
                let p = Pos::new(x, y);
                if let Some(bp) = state.get(p) {
                    pieces.push(PieceData { x, y, owner: bp.owner, piece_type: bp.piece_type });
                }
            }
        }
        pieces.sort_by(|a, b| a.y.cmp(&b.y).then(a.x.cmp(&b.x)));
        InitialData {
            pieces,
            hands: HandsData {
                attacker: HandCount::from_array(&state.hands.attacker),
                defender: HandCount::from_array(&state.hands.defender),
            },
            side_to_move: state.side_to_move,
        }
    }

    pub fn mirror(&self) -> Self {
        InitialData {
            pieces: self.pieces.iter().map(|p| PieceData {
                x: 10 - p.x,
                y: p.y,
                owner: p.owner,
                piece_type: p.piece_type,
            }).collect(),
            hands: self.hands.clone(),
            side_to_move: self.side_to_move,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ヘルパー ---

    fn empty_hands_data() -> HandsData {
        HandsData { attacker: HandCount::default(), defender: HandCount::default() }
    }

    /// 最小限の局面を作る: 攻め方玉(ax,ay), 守り方玉(dx,dy), 追加駒リスト
    fn make_state(ak: (i8, i8), dk: (i8, i8), extra: &[(i8, i8, Owner, PieceType)]) -> State {
        let mut pieces = vec![
            PieceData { x: ak.0, y: ak.1, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: dk.0, y: dk.1, owner: Owner::Defender, piece_type: PieceType::K },
        ];
        for &(x, y, owner, pt) in extra {
            pieces.push(PieceData { x, y, owner, piece_type: pt });
        }
        let init = InitialData { pieces, hands: empty_hands_data(), side_to_move: Owner::Attacker };
        init.to_state()
    }

    fn make_state_with_hands(ak: (i8, i8), dk: (i8, i8), extra: &[(i8, i8, Owner, PieceType)], hands: HandsData) -> State {
        let mut pieces = vec![
            PieceData { x: ak.0, y: ak.1, owner: Owner::Attacker, piece_type: PieceType::K },
            PieceData { x: dk.0, y: dk.1, owner: Owner::Defender, piece_type: PieceType::K },
        ];
        for &(x, y, owner, pt) in extra {
            pieces.push(PieceData { x, y, owner, piece_type: pt });
        }
        let init = InitialData { pieces, hands, side_to_move: Owner::Attacker };
        init.to_state()
    }

    // --- PieceType テスト ---

    #[test]
    fn test_promote_unpromote() {
        let promotable = [PieceType::R, PieceType::B, PieceType::S, PieceType::N, PieceType::L, PieceType::P];
        let promoted  = [PieceType::PR, PieceType::PB, PieceType::PS, PieceType::PN, PieceType::PL, PieceType::PP];
        for (base, prom) in promotable.iter().zip(promoted.iter()) {
            assert!(base.is_promotable());
            assert_eq!(base.promote(), *prom);
            assert_eq!(prom.unpromote(), *base);
            assert!(!base.is_promoted());
            assert!(prom.is_promoted());
        }
    }

    #[test]
    fn test_non_promotable() {
        assert!(!PieceType::K.is_promotable());
        assert!(!PieceType::G.is_promotable());
        assert_eq!(PieceType::K.promote(), PieceType::K);
        assert_eq!(PieceType::G.promote(), PieceType::G);
    }

    #[test]
    fn test_is_hand_type() {
        for &t in &HAND_TYPES {
            assert!(t.is_hand_type());
        }
        assert!(!PieceType::K.is_hand_type());
        assert!(!PieceType::PR.is_hand_type());
    }

    // --- Owner テスト ---

    #[test]
    fn test_owner_opposite() {
        assert_eq!(Owner::Attacker.opposite(), Owner::Defender);
        assert_eq!(Owner::Defender.opposite(), Owner::Attacker);
    }

    // --- Pos テスト ---

    #[test]
    fn test_pos_validity() {
        assert!(Pos::new(1, 1).is_valid());
        assert!(Pos::new(9, 9).is_valid());
        assert!(!Pos::new(0, 1).is_valid());
        assert!(!Pos::new(1, 10).is_valid());
    }

    // --- Hands テスト ---

    #[test]
    fn test_hands_operations() {
        let mut h = Hands::empty();
        assert_eq!(h.get(Owner::Attacker, PieceType::P), 0);
        h.add(Owner::Attacker, PieceType::P, 3);
        assert_eq!(h.get(Owner::Attacker, PieceType::P), 3);
        h.sub(Owner::Attacker, PieceType::P, 1);
        assert_eq!(h.get(Owner::Attacker, PieceType::P), 2);
        // saturating_sub で 0 以下にならない
        h.sub(Owner::Attacker, PieceType::P, 10);
        assert_eq!(h.get(Owner::Attacker, PieceType::P), 0);
    }

    #[test]
    fn test_hands_each_type() {
        let mut h = Hands::empty();
        for &t in &HAND_TYPES {
            h.add(Owner::Attacker, t, 1);
            assert_eq!(h.get(Owner::Attacker, t), 1);
            assert_eq!(h.get(Owner::Defender, t), 0);
        }
    }

    // --- 駒の動き テスト ---

    #[test]
    fn test_gold_moves() {
        // 中央に金を置いて合法手を確認（6方向に動ける）
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::G)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        assert_eq!(moves.len(), 6); // 金は6方向
    }

    #[test]
    fn test_silver_moves() {
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::S)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        // 銀は5方向、成りゾーン外なので成りなし
        assert_eq!(moves.len(), 5);
    }

    #[test]
    fn test_knight_moves() {
        // 桂馬を5,5に置く（攻め方）→ (4,3)と(6,3)に跳べる
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::N)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        let targets: Vec<(i8, i8)> = moves.iter().map(|m| (m.to[0], m.to[1])).collect();
        assert!(targets.contains(&(4, 3)));
        assert!(targets.contains(&(6, 3)));
        // 3段目に入るので成りも生成される
        assert!(moves.iter().any(|m| m.promote));
    }

    #[test]
    fn test_knight_must_promote_at_1_2_row() {
        // 桂馬を5,4に置く → (4,2)と(6,2)に跳べるが、2段目なので成り必須
        let state = make_state((5, 9), (5, 1), &[(5, 4, Owner::Attacker, PieceType::N)]);
        let piece = state.get(Pos::new(5, 4)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 4), piece);
        // 全ての手が成りでなければならない
        for m in &moves {
            assert!(m.promote, "桂馬が2段目に不成で移動できてしまう: {:?}", m);
        }
    }

    #[test]
    fn test_lance_slides_forward() {
        // 香車を5,5に置く → 前方(5,4), (5,3), (5,2)まで進める(5,1に玉がある)
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::L)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        let targets: Vec<(i8, i8)> = moves.iter().map(|m| (m.to[0], m.to[1])).collect();
        assert!(targets.contains(&(5, 4)));
        assert!(targets.contains(&(5, 3)));
        assert!(targets.contains(&(5, 2)));
        assert!(targets.contains(&(5, 1))); // 守り方玉を取れる
        assert!(!targets.contains(&(5, 6))); // 後ろには進めない
    }

    #[test]
    fn test_lance_must_promote_at_row_1() {
        // 香車を5,2に置く → (5,1)に進むときは成り必須
        let state = make_state((5, 9), (9, 1), &[(5, 2, Owner::Attacker, PieceType::L)]);
        let piece = state.get(Pos::new(5, 2)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 2), piece);
        let to_1 = moves.iter().filter(|m| m.to[1] == 1).collect::<Vec<_>>();
        assert!(!to_1.is_empty());
        for m in &to_1 {
            assert!(m.promote, "香車が1段目に不成で移動できてしまう");
        }
    }

    #[test]
    fn test_lance_blocked_by_piece() {
        // 香車を5,5に置き、5,3に味方の駒 → 5,4までしか進めない
        let state = make_state((5, 9), (5, 1), &[
            (5, 5, Owner::Attacker, PieceType::L),
            (5, 3, Owner::Attacker, PieceType::P),
        ]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        let targets: Vec<(i8, i8)> = moves.iter().map(|m| (m.to[0], m.to[1])).collect();
        assert!(targets.contains(&(5, 4)));
        assert!(!targets.contains(&(5, 3))); // 味方駒でブロック
    }

    #[test]
    fn test_rook_slides() {
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::R)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        // 上4マス(5,1まで)+下3マス(5,8まで、5,9は味方玉)+左4+右4 = 4+3+4+4 = 15 目的地
        // 成りゾーンに入る手は成り/不成の2通りあるので、手の数はもう少し多い
        assert!(moves.len() >= 15);
    }

    #[test]
    fn test_promoted_rook_has_diagonal_steps() {
        let state = make_state((1, 9), (9, 1), &[(5, 5, Owner::Attacker, PieceType::PR)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        let targets: Vec<(i8, i8)> = moves.iter().map(|m| (m.to[0], m.to[1])).collect();
        // 龍は斜め1マスも動ける
        assert!(targets.contains(&(4, 4)));
        assert!(targets.contains(&(6, 4)));
        assert!(targets.contains(&(4, 6)));
        assert!(targets.contains(&(6, 6)));
    }

    #[test]
    fn test_promoted_bishop_has_orthogonal_steps() {
        let state = make_state((1, 9), (9, 1), &[(5, 5, Owner::Attacker, PieceType::PB)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        let targets: Vec<(i8, i8)> = moves.iter().map(|m| (m.to[0], m.to[1])).collect();
        // 馬は十字1マスも動ける
        assert!(targets.contains(&(5, 4)));
        assert!(targets.contains(&(5, 6)));
        assert!(targets.contains(&(4, 5)));
        assert!(targets.contains(&(6, 5)));
    }

    #[test]
    fn test_promoted_knight_moves_like_gold() {
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::PN)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        assert_eq!(moves.len(), 6); // 金と同じ6方向
    }

    #[test]
    fn test_promoted_lance_moves_like_gold() {
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::PL)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        assert_eq!(moves.len(), 6);
    }

    #[test]
    fn test_pawn_moves_forward() {
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::P)]);
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].to, [5, 4]);
    }

    // --- 王手・合法手 テスト ---

    #[test]
    fn test_is_in_check() {
        // 飛車で王手
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::R)]);
        assert!(is_in_check(&state, Owner::Defender));
        assert!(!is_in_check(&state, Owner::Attacker));
    }

    #[test]
    fn test_not_in_check() {
        let state = make_state((5, 9), (5, 1), &[(3, 5, Owner::Attacker, PieceType::G)]);
        assert!(!is_in_check(&state, Owner::Defender));
    }

    #[test]
    fn test_knight_check() {
        // 桂馬で王手: 玉(5,1)、桂(4,3) → (5,1)に到達不可能 (桂の動きは(-1,-2),(1,-2))
        // 桂(6,3)なら (5,1)に到達可能 (6-1=5, 3-2=1)
        let state = make_state((5, 9), (5, 1), &[(6, 3, Owner::Attacker, PieceType::N)]);
        assert!(is_in_check(&state, Owner::Defender));
    }

    #[test]
    fn test_lance_check() {
        // 香車で王手: 玉(5,1)、香(5,5) → 前方直進で到達可能
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::L)]);
        assert!(is_in_check(&state, Owner::Defender));
    }

    #[test]
    fn test_legal_moves_no_self_check() {
        // 合法手は自玉を王手に晒さない手のみ
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::G)]);
        let moves = legal_moves(&state);
        for m in &moves {
            let next = apply_move(&state, m);
            assert!(!is_in_check(&next, Owner::Attacker));
        }
    }

    // --- 駒の取り・持ち駒 テスト ---

    #[test]
    fn test_capture_adds_to_hand() {
        // 金で敵の銀を取る → 持ち駒に銀が加わる
        let state = make_state((5, 9), (5, 1), &[
            (5, 5, Owner::Attacker, PieceType::G),
            (5, 4, Owner::Defender, PieceType::S),
        ]);
        let m = Move { from: Some([5, 5]), to: [5, 4], drop: None, promote: false };
        let next = apply_move(&state, &m);
        assert_eq!(next.hands.get(Owner::Attacker, PieceType::S), 1);
    }

    #[test]
    fn test_capture_promoted_adds_unpromoted() {
        // 成銀を取ったら持ち駒は銀
        let state = make_state((5, 9), (5, 1), &[
            (5, 5, Owner::Attacker, PieceType::G),
            (5, 4, Owner::Defender, PieceType::PS),
        ]);
        let m = Move { from: Some([5, 5]), to: [5, 4], drop: None, promote: false };
        let next = apply_move(&state, &m);
        assert_eq!(next.hands.get(Owner::Attacker, PieceType::S), 1);
    }

    // --- 打ち駒 テスト ---

    #[test]
    fn test_drop_pawn_restrictions() {
        // 歩は1段目に打てない（攻め方の場合）
        let mut hands = empty_hands_data();
        hands.attacker.P = 1;
        let state = make_state_with_hands((5, 9), (5, 1), &[], hands);
        let drops = pseudo_drops(&state, Owner::Attacker);
        let pawn_drops: Vec<_> = drops.iter().filter(|m| m.drop == Some(PieceType::P)).collect();
        assert!(pawn_drops.iter().all(|m| m.to[1] != 1), "歩が1段目に打ててしまう");
    }

    #[test]
    fn test_drop_lance_restrictions() {
        let mut hands = empty_hands_data();
        hands.attacker.L = 1;
        let state = make_state_with_hands((5, 9), (5, 1), &[], hands);
        let drops = pseudo_drops(&state, Owner::Attacker);
        let lance_drops: Vec<_> = drops.iter().filter(|m| m.drop == Some(PieceType::L)).collect();
        assert!(lance_drops.iter().all(|m| m.to[1] != 1), "香が1段目に打てしまう");
    }

    #[test]
    fn test_drop_knight_restrictions() {
        let mut hands = empty_hands_data();
        hands.attacker.N = 1;
        let state = make_state_with_hands((5, 9), (5, 1), &[], hands);
        let drops = pseudo_drops(&state, Owner::Attacker);
        let knight_drops: Vec<_> = drops.iter().filter(|m| m.drop == Some(PieceType::N)).collect();
        assert!(knight_drops.iter().all(|m| m.to[1] > 2), "桂が1〜2段目に打てしまう");
    }

    #[test]
    fn test_nifu_prohibition() {
        // 二歩禁止: 同じ筋に歩があると打てない
        let mut hands = empty_hands_data();
        hands.attacker.P = 1;
        let state = make_state_with_hands(
            (5, 9), (5, 1),
            &[(3, 5, Owner::Attacker, PieceType::P)],
            hands,
        );
        let drops = pseudo_drops(&state, Owner::Attacker);
        let pawn_3 = drops.iter().filter(|m| m.drop == Some(PieceType::P) && m.to[0] == 3).count();
        assert_eq!(pawn_3, 0, "二歩が許可されてしまう");
    }

    #[test]
    fn test_pawn_drop_mate_forbidden() {
        // 打ち歩詰め禁止テスト
        // 守り方玉(5,1)、攻め方金(4,1)(6,1)(5,2)で囲い、攻め方が歩を(5,1)の上に打って詰み→禁止
        // ただし実際に打ち歩詰めの局面を作るのは複雑なので、関数の存在確認のみ
        let mut hands = empty_hands_data();
        hands.attacker.P = 1;
        let state = make_state_with_hands(
            (5, 9), (5, 1),
            &[
                (4, 1, Owner::Attacker, PieceType::G),
                (6, 1, Owner::Attacker, PieceType::G),
                (4, 2, Owner::Attacker, PieceType::G),
                (6, 2, Owner::Attacker, PieceType::G),
            ],
            hands,
        );
        let legal = legal_moves(&state);
        // 歩を5,2に打つと打ち歩詰め → その手は合法手に含まれないはず
        let pawn_drop_52 = legal.iter().find(|m| m.drop == Some(PieceType::P) && m.to == [5, 2]);
        assert!(pawn_drop_52.is_none(), "打ち歩詰めが許可されてしまう");
    }

    // --- apply_move テスト ---

    #[test]
    fn test_apply_move_changes_side() {
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::G)]);
        let m = Move { from: Some([5, 5]), to: [5, 4], drop: None, promote: false };
        let next = apply_move(&state, &m);
        assert_eq!(next.side_to_move, Owner::Defender);
    }

    #[test]
    fn test_apply_move_promotion() {
        // 銀を3段目に移動して成る
        let state = make_state((5, 9), (5, 1), &[(5, 4, Owner::Attacker, PieceType::S)]);
        let m = Move { from: Some([5, 4]), to: [5, 3], drop: None, promote: true };
        let next = apply_move(&state, &m);
        let piece = next.get(Pos::new(5, 3)).unwrap();
        assert_eq!(piece.piece_type, PieceType::PS);
    }

    #[test]
    fn test_apply_drop() {
        let mut hands = empty_hands_data();
        hands.attacker.G = 1;
        let state = make_state_with_hands((5, 9), (5, 1), &[], hands);
        let m = Move { from: None, to: [5, 5], drop: Some(PieceType::G), promote: false };
        let next = apply_move(&state, &m);
        assert_eq!(next.hands.get(Owner::Attacker, PieceType::G), 0);
        let piece = next.get(Pos::new(5, 5)).unwrap();
        assert_eq!(piece.piece_type, PieceType::G);
        assert_eq!(piece.owner, Owner::Attacker);
    }

    // --- 詰め判定テスト ---

    #[test]
    fn test_validate_one_move_mate() {
        // 一手詰め: 守り方玉(1,1)、銀(2,1)が逃げ道を塞ぐ
        // 持ち駒の金を(1,2)に打って詰み
        // 銀(2,1)が(1,2)を守るので玉は金を取れない
        let mut hands = empty_hands_data();
        hands.attacker.G = 1;
        let state = make_state_with_hands((5, 9), (1, 1), &[
            (2, 1, Owner::Attacker, PieceType::S),
        ], hands);
        assert!(!is_in_check(&state, Owner::Defender), "初期局面で王手がかかっている");
        let result = validate_tsume_puzzle(&state, 1);
        assert!(result.is_some(), "一手詰めが検出されない");
    }

    #[test]
    fn test_validate_rejects_already_in_check() {
        // 初期状態で既に王手 → 無効
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::R)]);
        assert!(is_in_check(&state, Owner::Defender));
        let result = validate_tsume_puzzle(&state, 1);
        assert!(result.is_none(), "初期局面で王手の問題が通ってしまう");
    }

    #[test]
    fn test_validate_rejects_even_mate_length() {
        let state = make_state((5, 9), (5, 1), &[]);
        assert!(validate_tsume_puzzle(&state, 2).is_none());
        assert!(validate_tsume_puzzle(&state, 0).is_none());
    }

    // --- InitialData 変換テスト ---

    #[test]
    fn test_state_roundtrip() {
        let init = InitialData {
            pieces: vec![
                PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
                PieceData { x: 5, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
                PieceData { x: 3, y: 5, owner: Owner::Attacker, piece_type: PieceType::N },
            ],
            hands: empty_hands_data(),
            side_to_move: Owner::Attacker,
        };
        let state = init.to_state();
        let back = InitialData::from_state(&state);
        assert_eq!(back.pieces.len(), 3);
        assert_eq!(back.side_to_move, Owner::Attacker);
    }

    #[test]
    fn test_mirror() {
        let init = InitialData {
            pieces: vec![
                PieceData { x: 2, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
                PieceData { x: 5, y: 9, owner: Owner::Attacker, piece_type: PieceType::K },
            ],
            hands: empty_hands_data(),
            side_to_move: Owner::Attacker,
        };
        let mirrored = init.mirror();
        let dk = mirrored.pieces.iter().find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K).unwrap();
        assert_eq!(dk.x, 8); // 10 - 2 = 8
        assert_eq!(dk.y, 1);
    }

    // --- JSON シリアライズテスト ---

    #[test]
    fn test_piece_type_serialize() {
        let json = serde_json::to_string(&PieceType::PR).unwrap();
        assert_eq!(json, "\"+R\"");
        let json = serde_json::to_string(&PieceType::PN).unwrap();
        assert_eq!(json, "\"+N\"");
        let json = serde_json::to_string(&PieceType::PL).unwrap();
        assert_eq!(json, "\"+L\"");
    }

    #[test]
    fn test_piece_type_deserialize() {
        let pt: PieceType = serde_json::from_str("\"+N\"").unwrap();
        assert_eq!(pt, PieceType::PN);
        let pt: PieceType = serde_json::from_str("\"L\"").unwrap();
        assert_eq!(pt, PieceType::L);
    }

    // --- 守り方(Defender)の駒の動き テスト ---

    #[test]
    fn test_defender_pawn_moves_down() {
        // 守り方の歩は下方向(+y)に動く
        let mut state = make_state((5, 9), (5, 1), &[(3, 5, Owner::Defender, PieceType::P)]);
        state.side_to_move = Owner::Defender;
        let piece = state.get(Pos::new(3, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(3, 5), piece);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].to, [3, 6]);
    }

    #[test]
    fn test_defender_knight_jumps_down() {
        let mut state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Defender, PieceType::N)]);
        state.side_to_move = Owner::Defender;
        let piece = state.get(Pos::new(5, 5)).unwrap();
        let moves = pseudo_moves_from(&state, Pos::new(5, 5), piece);
        let targets: Vec<(i8, i8)> = moves.iter().map(|m| (m.to[0], m.to[1])).collect();
        // 守り方の桂は下方向にジャンプ: (4,7),(6,7)
        assert!(targets.contains(&(4, 7)));
        assert!(targets.contains(&(6, 7)));
    }
}
