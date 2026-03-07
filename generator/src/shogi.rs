use std::collections::HashMap;
use serde::{Deserialize, Serialize};

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
                if t == PieceType::P {
                    if has_pawn_on_file(state, owner, x) { continue; }
                }
                out.push(Move { from: None, to: [x, y], drop: Some(t), promote: false });
            }
        }
    }
    out
}

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

pub fn is_in_check(state: &State, owner: Owner) -> bool {
    let kp = match state.king_pos(owner) {
        Some(p) => p,
        None => return true,
    };
    is_square_attacked(state, kp, owner.opposite())
}

/// Check if the side to move has at least one legal move (faster than generating all)
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
        if m.drop == Some(PieceType::P) {
            let next = apply_move(state, &m);
            let enemy = owner.opposite();
            if is_in_check(&next, enemy) && !has_any_legal_move(&next) {
                continue; // pawn drop mate forbidden
            }
        }
        let next = apply_move(state, &m);
        if !is_in_check(&next, owner) {
            return true;
        }
    }

    false
}

fn pawn_drop_mate_forbidden(state: &State, m: &Move) -> bool {
    if m.drop != Some(PieceType::P) { return false; }
    let owner = state.side_to_move;
    let next = apply_move(state, m);
    let enemy = owner.opposite();
    if !is_in_check(&next, enemy) { return false; }
    !has_any_legal_move(&next)
}

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

            if all_mate && best_move.is_some() {
                let mut l = vec![best_move.unwrap()];
                l.extend(best_line);
                MateResult { mate: true, unique: all_unique, line: l }
            } else {
                MateResult { mate: false, unique: false, line: vec![] }
            }
        }
    } else {
        if plies == 0 {
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
        }
    };

    memo.insert(key, result.clone());
    result
}

pub fn validate_tsume_puzzle(state: &State, mate_length: u32) -> Option<Vec<Move>> {
    if mate_length % 2 == 0 || mate_length == 0 { return None; }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for HandCount {
    fn default() -> Self {
        HandCount { R: 0, B: 0, G: 0, S: 0, N: 0, L: 0, P: 0 }
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
        state.hands.attacker = [self.hands.attacker.R, self.hands.attacker.B, self.hands.attacker.G, self.hands.attacker.S, self.hands.attacker.N, self.hands.attacker.L, self.hands.attacker.P];
        state.hands.defender = [self.hands.defender.R, self.hands.defender.B, self.hands.defender.G, self.hands.defender.S, self.hands.defender.N, self.hands.defender.L, self.hands.defender.P];
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
                attacker: HandCount {
                    R: state.hands.attacker[0],
                    B: state.hands.attacker[1],
                    G: state.hands.attacker[2],
                    S: state.hands.attacker[3],
                    N: state.hands.attacker[4],
                    L: state.hands.attacker[5],
                    P: state.hands.attacker[6],
                },
                defender: HandCount {
                    R: state.hands.defender[0],
                    B: state.hands.defender[1],
                    G: state.hands.defender[2],
                    S: state.hands.defender[3],
                    N: state.hands.defender[4],
                    L: state.hands.defender[5],
                    P: state.hands.defender[6],
                },
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
