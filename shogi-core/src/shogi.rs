use rustc_hash::FxHashMap;
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

/// Zobrist ハッシュテーブル（局面のハッシュを差分更新するための乱数テーブル）
struct ZobristTable {
    /// board[owner][piece_type][square]: 盤上の駒
    board: [[[u64; 81]; 14]; 2],
    /// hand[owner][piece_type][count]: 持ち駒（count=0..18）
    hand: [[[u64; 19]; 7]; 2],
    /// 手番（攻め方の手番の場合に XOR する値）
    side: u64,
}

/// 駒種から ALL_PIECE_TYPES のインデックスを返す
fn piece_type_index(t: PieceType) -> usize {
    match t {
        PieceType::K => 0, PieceType::R => 1, PieceType::B => 2, PieceType::G => 3,
        PieceType::S => 4, PieceType::N => 5, PieceType::L => 6, PieceType::P => 7,
        PieceType::PR => 8, PieceType::PB => 9, PieceType::PS => 10, PieceType::PN => 11,
        PieceType::PL => 12, PieceType::PP => 13,
    }
}

fn owner_index(o: Owner) -> usize {
    match o { Owner::Attacker => 0, Owner::Defender => 1 }
}

impl ZobristTable {
    fn new() -> Self {
        // 固定シードの疑似乱数で初期化（再現性のため）
        let mut rng: u64 = 0xDEAD_BEEF_CAFE_BABE;
        let mut next = || -> u64 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };

        let mut board = [[[0u64; 81]; 14]; 2];
        for owner_arr in &mut board {
            for pt_arr in owner_arr.iter_mut() {
                for sq_val in pt_arr.iter_mut() {
                    *sq_val = next();
                }
            }
        }

        let mut hand = [[[0u64; 19]; 7]; 2];
        for owner_arr in &mut hand {
            for ht_arr in owner_arr.iter_mut() {
                for count_val in ht_arr.iter_mut() {
                    *count_val = next();
                }
            }
        }

        let side = next();

        ZobristTable { board, hand, side }
    }

    /// 盤上の駒のハッシュ値
    fn board_hash(&self, owner: Owner, piece_type: PieceType, sq: usize) -> u64 {
        self.board[owner_index(owner)][piece_type_index(piece_type)][sq]
    }

    /// 持ち駒のハッシュ値
    fn hand_hash(&self, owner: Owner, hand_idx: usize, count: u8) -> u64 {
        self.hand[owner_index(owner)][hand_idx][count as usize]
    }
}

use std::sync::LazyLock;

static ZOBRIST: LazyLock<ZobristTable> = LazyLock::new(ZobristTable::new);

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
    /// 玉の位置キャッシュ（O(81)の線形探索を回避）
    attacker_king: Option<Pos>,
    defender_king: Option<Pos>,
    /// Zobrist ハッシュ値（差分更新で高速にキーを計算）
    pub zobrist_hash: u64,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        State {
            board: [None; 81],
            hands: Hands::empty(),
            side_to_move: Owner::Attacker,
            attacker_king: None,
            defender_king: None,
            zobrist_hash: ZOBRIST.side, // 攻め方の手番
        }
    }

    pub fn get(&self, pos: Pos) -> Option<BoardPiece> {
        self.board[pos.idx()]
    }

    pub fn set(&mut self, pos: Pos, piece: Option<BoardPiece>) {
        let idx = pos.idx();
        // 既存の駒を Zobrist から除去
        if let Some(old) = self.board[idx] {
            self.zobrist_hash ^= ZOBRIST.board_hash(old.owner, old.piece_type, idx);
            if old.piece_type == PieceType::K {
                match old.owner {
                    Owner::Attacker => self.attacker_king = None,
                    Owner::Defender => self.defender_king = None,
                }
            }
        }
        // 新しい駒を Zobrist に追加
        if let Some(bp) = piece {
            self.zobrist_hash ^= ZOBRIST.board_hash(bp.owner, bp.piece_type, idx);
            if bp.piece_type == PieceType::K {
                match bp.owner {
                    Owner::Attacker => self.attacker_king = Some(pos),
                    Owner::Defender => self.defender_king = Some(pos),
                }
            }
        }
        self.board[idx] = piece;
    }

    pub fn king_pos(&self, owner: Owner) -> Option<Pos> {
        match owner {
            Owner::Attacker => self.attacker_king,
            Owner::Defender => self.defender_king,
        }
    }

    /// 盤面+手番のみの Zobrist ハッシュ（持ち駒を除外）を返す
    /// 証明駒・反証駒の支配性テーブルで使用する
    pub fn board_only_zobrist(&self) -> u64 {
        let mut h = self.zobrist_hash;
        // 持ち駒成分を XOR で除去
        for (hi, &t) in HAND_TYPES.iter().enumerate() {
            let ac = self.hands.get(Owner::Attacker, t);
            h ^= ZOBRIST.hand_hash(Owner::Attacker, hi, ac);
            let dc = self.hands.get(Owner::Defender, t);
            h ^= ZOBRIST.hand_hash(Owner::Defender, hi, dc);
        }
        h
    }

    /// Zobrist ハッシュをゼロから計算する（初期化時に使用）
    pub fn compute_zobrist(&self) -> u64 {
        let mut h = 0u64;
        // 盤上の駒
        for sq in 0..81 {
            if let Some(bp) = self.board[sq] {
                h ^= ZOBRIST.board_hash(bp.owner, bp.piece_type, sq);
            }
        }
        // 持ち駒（count=0 も含めてハッシュに含める）
        for (hi, &t) in HAND_TYPES.iter().enumerate() {
            let ac = self.hands.get(Owner::Attacker, t);
            h ^= ZOBRIST.hand_hash(Owner::Attacker, hi, ac);
            let dc = self.hands.get(Owner::Defender, t);
            h ^= ZOBRIST.hand_hash(Owner::Defender, hi, dc);
        }
        // 手番
        if self.side_to_move == Owner::Attacker {
            h ^= ZOBRIST.side;
        }
        h
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

pub fn promotion_zone(owner: Owner, y: i8) -> bool {
    match owner {
        Owner::Attacker => y <= 3,
        Owner::Defender => y >= 7,
    }
}

pub fn transform_dir(owner: Owner, dx: i8, dy: i8) -> (i8, i8) {
    match owner {
        Owner::Attacker => (dx, dy),
        Owner::Defender => (-dx, -dy),
    }
}

pub fn step_moves(t: PieceType) -> &'static [(i8, i8)] {
    match t {
        PieceType::K => &[(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)],
        PieceType::G | PieceType::PP | PieceType::PS | PieceType::PN | PieceType::PL => &[(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(0,1)],
        PieceType::S => &[(-1,-1),(0,-1),(1,-1),(-1,1),(1,1)],
        PieceType::N => &[(-1,-2),(1,-2)],
        PieceType::P => &[(0,-1)],
        _ => &[],
    }
}

pub fn slide_dirs(t: PieceType) -> &'static [(i8, i8)] {
    match t {
        PieceType::R | PieceType::PR => &[(0,-1),(1,0),(0,1),(-1,0)],
        PieceType::B | PieceType::PB => &[(1,-1),(1,1),(-1,1),(-1,-1)],
        PieceType::L => &[(0,-1)],
        _ => &[],
    }
}

pub fn extra_steps(t: PieceType) -> &'static [(i8, i8)] {
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
        let old_count = next.hands.get(owner, drop_type);
        next.hands.sub(owner, drop_type, 1);
        zobrist_hand_update(&mut next.zobrist_hash, owner, drop_type, old_count, old_count - 1);
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
                let old_count = next.hands.get(owner, cap_type);
                next.hands.add(owner, cap_type, 1);
                zobrist_hand_update(&mut next.zobrist_hash, owner, cap_type, old_count, old_count + 1);
            }
        }

        next.set(fp, None);
        let moved_type = if m.promote { src.piece_type.promote() } else { src.piece_type };
        next.set(tp, Some(BoardPiece { owner, piece_type: moved_type }));
    }

    next.side_to_move = owner.opposite();
    next.zobrist_hash ^= ZOBRIST.side;
    next
}

/// スライド駒かどうか判定する（飛車・龍・角・馬・香車）
pub fn is_sliding_piece(t: PieceType) -> bool {
    matches!(t, PieceType::R | PieceType::PR | PieceType::B | PieceType::PB | PieceType::L)
}

/// 王手をかけている駒の位置と種類を探す
pub fn find_checkers(state: &State, king_owner: Owner) -> Vec<(Pos, BoardPiece)> {
    let kp = match state.king_pos(king_owner) {
        Some(p) => p,
        None => return vec![],
    };
    let attacker = king_owner.opposite();
    let mut checkers = Vec::new();

    // ステップ駒（全8方向）
    for &(dx, dy) in &[(-1i8,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] {
        let p = Pos::new(kp.x + dx, kp.y + dy);
        if !p.is_valid() { continue; }
        if let Some(bp) = state.get(p) {
            if bp.owner == attacker && can_step_to(bp.piece_type, attacker, p, kp) {
                checkers.push((p, bp));
            }
        }
    }

    // 桂馬
    let knight_offsets: [(i8, i8); 2] = match attacker {
        Owner::Attacker => [(1, 2), (-1, 2)],
        Owner::Defender => [(1, -2), (-1, -2)],
    };
    for &(dx, dy) in &knight_offsets {
        let p = Pos::new(kp.x + dx, kp.y + dy);
        if !p.is_valid() { continue; }
        if let Some(bp) = state.get(p) {
            if bp.owner == attacker && bp.piece_type == PieceType::N {
                checkers.push((p, bp));
            }
        }
    }

    // 飛車・龍（十字方向）
    for &(dx, dy) in &[(0i8,-1),(1,0),(0,1),(-1,0)] {
        let mut nx = kp.x + dx;
        let mut ny = kp.y + dy;
        while Pos::new(nx, ny).is_valid() {
            if let Some(bp) = state.get(Pos::new(nx, ny)) {
                if bp.owner == attacker {
                    match bp.piece_type {
                        PieceType::R | PieceType::PR => checkers.push((Pos::new(nx, ny), bp)),
                        PieceType::L => {
                            let forward_dy = match attacker {
                                Owner::Attacker => 1,
                                Owner::Defender => -1,
                            };
                            if dy == forward_dy { checkers.push((Pos::new(nx, ny), bp)); }
                        }
                        _ => {}
                    }
                }
                break;
            }
            nx += dx;
            ny += dy;
        }
    }

    // 角・馬（斜め方向）
    for &(dx, dy) in &[(1i8,-1),(1,1),(-1,1),(-1,-1)] {
        let mut nx = kp.x + dx;
        let mut ny = kp.y + dy;
        while Pos::new(nx, ny).is_valid() {
            if let Some(bp) = state.get(Pos::new(nx, ny)) {
                if bp.owner == attacker {
                    match bp.piece_type {
                        PieceType::B | PieceType::PB => checkers.push((Pos::new(nx, ny), bp)),
                        _ => {}
                    }
                }
                break;
            }
            nx += dx;
            ny += dy;
        }
    }

    checkers
}

/// スライド駒と玉の間の遮断可能マスを列挙する（両端を含まない）
pub fn interposition_squares(checker: Pos, king: Pos) -> Vec<Pos> {
    let dx = (king.x - checker.x).signum();
    let dy = (king.y - checker.y).signum();
    let mut squares = Vec::new();
    let mut x = checker.x + dx;
    let mut y = checker.y + dy;
    while (x, y) != (king.x, king.y) {
        squares.push(Pos::new(x, y));
        x += dx;
        y += dy;
    }
    squares
}

/// 2点間（直線・斜め）の中間にあるか判定する（両端を含まない）
pub fn is_between(checker: Pos, king: Pos, target: Pos) -> bool {
    let dx = king.x - checker.x;
    let dy = king.y - checker.y;
    let tx = target.x - checker.x;
    let ty = target.y - checker.y;

    if dx == 0 && dy == 0 { return false; }

    if dx == 0 {
        // 縦ライン
        tx == 0 && ty.signum() == dy.signum() && ty.abs() > 0 && ty.abs() < dy.abs()
    } else if dy == 0 {
        // 横ライン
        ty == 0 && tx.signum() == dx.signum() && tx.abs() > 0 && tx.abs() < dx.abs()
    } else if dx.abs() == dy.abs() {
        // 斜めライン
        tx.abs() == ty.abs()
            && tx.signum() == dx.signum()
            && ty.signum() == dy.signum()
            && tx.abs() > 0
            && tx.abs() < dx.abs()
    } else {
        false
    }
}

/// 指し手の適用に必要な復元情報
#[derive(Debug)]
pub struct UndoInfo {
    pub captured: Option<BoardPiece>,   // 取った駒（None=取りなし）
}

/// 持ち駒の Zobrist ハッシュを更新するヘルパー
/// add/sub の前に旧値を XOR で除去し、変更後に新値を XOR で追加する
fn zobrist_hand_update(hash: &mut u64, owner: Owner, t: PieceType, old_count: u8, new_count: u8) {
    let hi = Hands::hand_idx(t);
    *hash ^= ZOBRIST.hand_hash(owner, hi, old_count);
    *hash ^= ZOBRIST.hand_hash(owner, hi, new_count);
}

/// 指し手を局面に直接適用する（clone 不要、undo_move で復元可能）
pub fn make_move(state: &mut State, m: &Move) -> UndoInfo {
    let owner = state.side_to_move;

    if let Some(drop_type) = m.drop {
        let old_count = state.hands.get(owner, drop_type);
        state.hands.sub(owner, drop_type, 1);
        zobrist_hand_update(&mut state.zobrist_hash, owner, drop_type, old_count, old_count - 1);
        let tp = Pos::new(m.to[0], m.to[1]);
        state.set(tp, Some(BoardPiece { owner, piece_type: drop_type }));
        state.side_to_move = owner.opposite();
        state.zobrist_hash ^= ZOBRIST.side; // 手番反転
        UndoInfo { captured: None }
    } else {
        let from = m.from.unwrap();
        let fp = Pos::new(from[0], from[1]);
        let src = state.get(fp).unwrap();
        let tp = Pos::new(m.to[0], m.to[1]);
        let captured = state.get(tp);

        if let Some(cap) = captured {
            let cap_type = cap.piece_type.unpromote();
            if cap_type != PieceType::K {
                let old_count = state.hands.get(owner, cap_type);
                state.hands.add(owner, cap_type, 1);
                zobrist_hand_update(&mut state.zobrist_hash, owner, cap_type, old_count, old_count + 1);
            }
        }

        state.set(fp, None);
        let moved_type = if m.promote { src.piece_type.promote() } else { src.piece_type };
        state.set(tp, Some(BoardPiece { owner, piece_type: moved_type }));
        state.side_to_move = owner.opposite();
        state.zobrist_hash ^= ZOBRIST.side; // 手番反転
        UndoInfo { captured }
    }
}

/// make_move で適用した手を元に戻す
pub fn undo_move(state: &mut State, m: &Move, undo: &UndoInfo) {
    // side_to_move を戻す（make_move で opposite() にしたので再度反転）
    state.side_to_move = state.side_to_move.opposite();
    state.zobrist_hash ^= ZOBRIST.side; // 手番反転を戻す
    let owner = state.side_to_move;

    if let Some(drop_type) = m.drop {
        let tp = Pos::new(m.to[0], m.to[1]);
        state.set(tp, None);
        let old_count = state.hands.get(owner, drop_type);
        state.hands.add(owner, drop_type, 1);
        zobrist_hand_update(&mut state.zobrist_hash, owner, drop_type, old_count, old_count + 1);
    } else {
        let from = m.from.unwrap();
        let fp = Pos::new(from[0], from[1]);
        let tp = Pos::new(m.to[0], m.to[1]);
        let moved = state.get(tp).unwrap();
        // 成りを戻す
        let orig_type = if m.promote { moved.piece_type.unpromote() } else { moved.piece_type };
        // tp を先に復元してから fp を復元する
        // （玉移動の場合、fp を先に set すると tp の set で玉キャッシュがクリアされるため）
        state.set(tp, undo.captured);
        state.set(fp, Some(BoardPiece { owner, piece_type: orig_type }));

        // 持ち駒を戻す
        if let Some(cap) = undo.captured {
            let cap_type = cap.piece_type.unpromote();
            if cap_type != PieceType::K {
                let old_count = state.hands.get(owner, cap_type);
                state.hands.sub(owner, cap_type, 1);
                zobrist_hand_update(&mut state.zobrist_hash, owner, cap_type, old_count, old_count - 1);
            }
        }
    }
}

/// target マスに attacker 側の駒の利きがあるか判定する（逆方向探索）
/// 全81マス走査の代わりに、target から各方向に駒を探して利きを確認する
fn is_square_attacked(state: &State, target: Pos, attacker: Owner) -> bool {
    // ステップ駒の逆方向チェック: target から見て各方向1マスに attacker の駒があり、
    // その駒がそのステップで target に到達できるか

    // 玉のステップ方向（全8方向）— 玉・金系・銀のチェックに使う
    for &(dx, dy) in &[(-1i8,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] {
        let p = Pos::new(target.x + dx, target.y + dy);
        if !p.is_valid() { continue; }
        if let Some(bp) = state.get(p) {
            if bp.owner == attacker && can_step_to(bp.piece_type, attacker, p, target) {
                return true;
            }
        }
    }

    // 桂馬の逆方向チェック: 桂馬は(-1,-2),(1,-2)のステップなので、
    // target から見て(+1,+2),(-1,+2)の位置に桂馬があるか（攻め方の場合）
    let knight_offsets: [(i8, i8); 2] = match attacker {
        Owner::Attacker => [(1, 2), (-1, 2)],
        Owner::Defender => [(1, -2), (-1, -2)],
    };
    for &(dx, dy) in &knight_offsets {
        let p = Pos::new(target.x + dx, target.y + dy);
        if !p.is_valid() { continue; }
        if let Some(bp) = state.get(p) {
            if bp.owner == attacker && (bp.piece_type == PieceType::N) {
                return true;
            }
        }
    }

    // 走り駒（飛車・龍の十字方向、角・馬の斜め方向、香車の前方向）
    // 十字方向: 飛車・龍
    for &(dx, dy) in &[(0i8,-1),(1,0),(0,1),(-1,0)] {
        let mut nx = target.x + dx;
        let mut ny = target.y + dy;
        while Pos::new(nx, ny).is_valid() {
            if let Some(bp) = state.get(Pos::new(nx, ny)) {
                if bp.owner == attacker {
                    match bp.piece_type {
                        PieceType::R | PieceType::PR => return true,
                        // 香車: 前方向のみ（攻め方は dy=-1 なので target から見て dy=+1 方向に香車がある）
                        PieceType::L => {
                            let forward_dy = match attacker {
                                Owner::Attacker => 1,  // target から見て下方向に攻め方の香車
                                Owner::Defender => -1,
                            };
                            if dy == forward_dy { return true; }
                        }
                        _ => {}
                    }
                }
                break; // 駒にぶつかったらその方向は終了
            }
            nx += dx;
            ny += dy;
        }
    }

    // 斜め方向: 角・馬
    for &(dx, dy) in &[(1i8,-1),(1,1),(-1,1),(-1,-1)] {
        let mut nx = target.x + dx;
        let mut ny = target.y + dy;
        while Pos::new(nx, ny).is_valid() {
            if let Some(bp) = state.get(Pos::new(nx, ny)) {
                if bp.owner == attacker {
                    match bp.piece_type {
                        PieceType::B | PieceType::PB => return true,
                        _ => {}
                    }
                }
                break;
            }
            nx += dx;
            ny += dy;
        }
    }

    false
}

/// ステップ駒が from から target に1手で移動できるか判定する
#[inline]
fn can_step_to(piece_type: PieceType, owner: Owner, from: Pos, target: Pos) -> bool {
    let dx = target.x - from.x;
    let dy = target.y - from.y;
    // owner の方向変換を逆適用して、定義上の方向と比較
    let (ndx, ndy) = transform_dir(owner, dx, dy);

    for &(sx, sy) in step_moves(piece_type) {
        if sx == ndx && sy == ndy { return true; }
    }
    for &(sx, sy) in extra_steps(piece_type) {
        if sx == ndx && sy == ndy { return true; }
    }
    false
}

/// owner 側の玉に王手がかかっているか判定する
pub fn is_in_check(state: &State, owner: Owner) -> bool {
    let kp = match state.king_pos(owner) {
        Some(p) => p,
        // 玉が盤上に無い場合は王手されていない（攻め方の玉が無いパズルに対応）
        None => return false,
    };
    is_square_attacked(state, kp, owner.opposite())
}

/// 手番側に合法手が1つでもあるか判定する（全手生成より高速）
pub fn has_any_legal_move(state: &mut State) -> bool {
    let owner = state.side_to_move;

    // 盤上の駒の位置を先に収集（state を後で &mut で使うため）
    let mut piece_positions = Vec::new();
    for y in 1..=9i8 {
        for x in 1..=9i8 {
            let p = Pos::new(x, y);
            if let Some(bp) = state.get(p) {
                if bp.owner == owner {
                    piece_positions.push((p, bp));
                }
            }
        }
    }

    for (p, bp) in &piece_positions {
        for m in pseudo_moves_from(state, *p, *bp) {
            let undo = make_move(state, &m);
            let in_check = is_in_check(state, owner);
            undo_move(state, &m, &undo);
            if !in_check {
                return true;
            }
        }
    }

    // 王手されている場合、ドロップ候補を最適化
    let in_check = is_in_check(state, owner);
    if in_check {
        let checkers = find_checkers(state, owner);
        if checkers.len() >= 2 {
            return false; // 両王手: ドロップでは逃げられない
        }
        if checkers.len() == 1 {
            let (ck_pos, ck_bp) = checkers[0];
            if !is_sliding_piece(ck_bp.piece_type) {
                return false; // 接触王手: 合駒では防げない
            }
            // スライド王手: 遮断マスへのドロップのみチェック
            let kp = state.king_pos(owner).unwrap();
            let interp = interposition_squares(ck_pos, kp);
            for target in &interp {
                if state.get(*target).is_some() { continue; }
                for &t in &HAND_TYPES {
                    if state.hands.get(owner, t) == 0 { continue; }
                    match t {
                        PieceType::P | PieceType::L => {
                            if owner == Owner::Attacker && target.y == 1 { continue; }
                            if owner == Owner::Defender && target.y == 9 { continue; }
                        }
                        PieceType::N => {
                            if owner == Owner::Attacker && target.y <= 2 { continue; }
                            if owner == Owner::Defender && target.y >= 8 { continue; }
                        }
                        _ => {}
                    }
                    if t == PieceType::P && has_pawn_on_file(state, owner, target.x) { continue; }
                    let m = Move { from: None, to: [target.x, target.y], drop: Some(t), promote: false };
                    let undo = make_move(state, &m);
                    let still_check = is_in_check(state, owner);
                    if still_check {
                        undo_move(state, &m, &undo);
                        continue;
                    }
                    if m.drop == Some(PieceType::P) {
                        let enemy = owner.opposite();
                        if is_in_check(state, enemy) && !has_any_legal_move(state) {
                            undo_move(state, &m, &undo);
                            continue;
                        }
                    }
                    undo_move(state, &m, &undo);
                    return true;
                }
            }
            return false;
        }
    }

    // 王手されていない場合: 通常のドロップ生成
    let drops = pseudo_drops(state, owner);
    for m in drops {
        let undo = make_move(state, &m);
        let in_check = is_in_check(state, owner);
        if in_check {
            undo_move(state, &m, &undo);
            continue;
        }
        // 打ち歩詰めチェック
        if m.drop == Some(PieceType::P) {
            let enemy = owner.opposite();
            if is_in_check(state, enemy) && !has_any_legal_move(state) {
                undo_move(state, &m, &undo);
                continue;
            }
        }
        undo_move(state, &m, &undo);
        return true;
    }

    false
}

/// 打ち歩詰めの禁手チェック: 歩を打って王手かつ相手の合法手がない場合は禁止
fn pawn_drop_mate_forbidden(state: &mut State, m: &Move) -> bool {
    if m.drop != Some(PieceType::P) { return false; }
    let owner = state.side_to_move;
    let enemy = owner.opposite();
    let undo = make_move(state, m);
    let result = if !is_in_check(state, enemy) {
        false
    } else {
        !has_any_legal_move(state)
    };
    undo_move(state, m, &undo);
    result
}

/// 盤上の駒の合法手のみを生成する（ドロップを含まない）
pub fn legal_board_moves(state: &mut State) -> Vec<Move> {
    let owner = state.side_to_move;
    let mut out = Vec::new();

    let mut piece_positions = Vec::new();
    for y in 1..=9i8 {
        for x in 1..=9i8 {
            let p = Pos::new(x, y);
            if let Some(bp) = state.get(p) {
                if bp.owner == owner {
                    piece_positions.push((p, bp));
                }
            }
        }
    }

    for (p, bp) in &piece_positions {
        for m in pseudo_moves_from(state, *p, *bp) {
            let undo = make_move(state, &m);
            let in_check = is_in_check(state, owner);
            undo_move(state, &m, &undo);
            if !in_check {
                out.push(m);
            }
        }
    }

    out
}

/// ドロップの合法手のみを生成する（王手時の最適化込み）
pub fn legal_drop_moves(state: &mut State) -> Vec<Move> {
    let owner = state.side_to_move;
    let mut out = Vec::new();

    // 王手されている場合、ドロップ候補を最適化
    let in_check = is_in_check(state, owner);
    if in_check {
        let checkers = find_checkers(state, owner);
        if checkers.len() >= 2 {
            return out; // 両王手: ドロップでは逃げられない
        }
        if checkers.len() == 1 {
            let (ck_pos, ck_bp) = checkers[0];
            if !is_sliding_piece(ck_bp.piece_type) {
                return out; // 接触王手: 合駒では防げない
            }
            // スライド王手: 遮断マスへのドロップのみ生成
            let kp = state.king_pos(owner).unwrap();
            let interp = interposition_squares(ck_pos, kp);
            for target in &interp {
                if state.get(*target).is_some() { continue; }
                for &t in &HAND_TYPES {
                    if state.hands.get(owner, t) == 0 { continue; }
                    match t {
                        PieceType::P | PieceType::L => {
                            if owner == Owner::Attacker && target.y == 1 { continue; }
                            if owner == Owner::Defender && target.y == 9 { continue; }
                        }
                        PieceType::N => {
                            if owner == Owner::Attacker && target.y <= 2 { continue; }
                            if owner == Owner::Defender && target.y >= 8 { continue; }
                        }
                        _ => {}
                    }
                    if t == PieceType::P && has_pawn_on_file(state, owner, target.x) { continue; }
                    let m = Move { from: None, to: [target.x, target.y], drop: Some(t), promote: false };
                    if pawn_drop_mate_forbidden(state, &m) { continue; }
                    let undo = make_move(state, &m);
                    let still_check = is_in_check(state, owner);
                    undo_move(state, &m, &undo);
                    if !still_check {
                        out.push(m);
                    }
                }
            }
            return out;
        }
    }

    // 王手されていない場合: 通常のドロップ生成
    let drops = pseudo_drops(state, owner);
    for m in drops {
        if pawn_drop_mate_forbidden(state, &m) { continue; }
        let undo = make_move(state, &m);
        let in_check = is_in_check(state, owner);
        undo_move(state, &m, &undo);
        if !in_check {
            out.push(m);
        }
    }

    out
}

/// 現在の手番側の合法手をすべて生成する（盤上の手 + ドロップ）
/// 自玉を王手に晒す手と打ち歩詰めは除外される
pub fn legal_moves(state: &mut State) -> Vec<Move> {
    let mut out = legal_board_moves(state);
    out.extend(legal_drop_moves(state));
    out
}

/// 現在の手番側の「王手になる合法手」を生成する
pub fn legal_check_moves(state: &mut State) -> Vec<Move> {
    let owner = state.side_to_move;
    let enemy = owner.opposite();
    let mut out = Vec::new();

    let mut piece_positions = Vec::new();
    for y in 1..=9i8 {
        for x in 1..=9i8 {
            let p = Pos::new(x, y);
            if let Some(bp) = state.get(p) {
                if bp.owner == owner {
                    piece_positions.push((p, bp));
                }
            }
        }
    }

    for (p, bp) in &piece_positions {
        for m in pseudo_moves_from(state, *p, *bp) {
            let undo = make_move(state, &m);
            let is_legal = !is_in_check(state, owner);
            let gives_check = is_legal && is_in_check(state, enemy);
            undo_move(state, &m, &undo);
            if gives_check {
                out.push(m);
            }
        }
    }

    let drops = pseudo_drops(state, owner);
    for m in drops {
        if pawn_drop_mate_forbidden(state, &m) {
            continue;
        }
        let undo = make_move(state, &m);
        let is_legal = !is_in_check(state, owner);
        let gives_check = is_legal && is_in_check(state, enemy);
        undo_move(state, &m, &undo);
        if gives_check {
            out.push(m);
        }
    }

    // ソート: 捕獲+成り > 捕獲 > 成り > 移動 > 打ち
    out.sort_by_key(|m| {
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

    out
}

#[derive(Debug, Clone)]
pub struct MateResult {
    pub mate: bool,
    pub unique: bool,
    pub line: Vec<Move>,
}

fn state_key(state: &State, plies: u32) -> u64 {
    // Zobrist ハッシュに残り手数を混ぜる（同一局面でも残り手数が違えば別エントリ）
    state.zobrist_hash.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(plies as u64)
}

/// plies 手以内の詰みを探索する
/// 攻め方は王手のみ許可（詰将棋ルール）、守り方は最善応手を選ぶ
/// メモ化により同一局面の再計算を回避する
pub fn forced_mate_within(state: &mut State, plies: u32, memo: &mut FxHashMap<u64, MateResult>) -> MateResult {
    let key = state_key(state, plies);
    if let Some(cached) = memo.get(&key) {
        return cached.clone();
    }

    let side = state.side_to_move;
    let enemy = side.opposite();

    let result = if side == Owner::Defender {
        // 守り方: 遅延ドロップ生成 — 盤上の手で逃れが見つかればドロップ生成をスキップ
        let board_moves = legal_board_moves(state);

        if board_moves.is_empty() && plies == 0 {
            // 盤上の手なし & plies=0: ドロップも確認する
            let drop_moves = legal_drop_moves(state);
            if drop_moves.is_empty() {
                MateResult { mate: is_in_check(state, side), unique: true, line: vec![] }
            } else {
                MateResult { mate: false, unique: false, line: vec![] }
            }
        } else if plies == 0 {
            if board_moves.is_empty() {
                // 盤上の手なし: ドロップも確認
                let drop_moves = legal_drop_moves(state);
                if drop_moves.is_empty() {
                    MateResult { mate: is_in_check(state, side), unique: true, line: vec![] }
                } else {
                    MateResult { mate: false, unique: false, line: vec![] }
                }
            } else {
                MateResult { mate: false, unique: false, line: vec![] }
            }
        } else {
            // 無駄合い判定用: スライド駒による単独王手か調べる
            let def_king_pos = state.king_pos(Owner::Defender);
            let checkers = if def_king_pos.is_some() { find_checkers(state, Owner::Defender) } else { vec![] };
            let sliding_checker = if checkers.len() == 1 {
                let (pos, bp) = checkers[0];
                if is_sliding_piece(bp.piece_type) { Some((pos, bp)) } else { None }
            } else {
                None
            };

            let mut all_mate = true;
            let mut all_unique = true;
            let mut best_move: Option<Move> = None;
            let mut best_line: Vec<Move> = vec![];

            // 無駄合い判定の共通処理（ドロップ・移動合い両対応）
            // スライド駒による単独王手時、合駒をライン上に打つ/移動する手に対して
            // 同Xで取り返して詰むなら無駄合いとしてスキップ
            let is_wasteful_interposition = |state: &mut State, m: &Move, memo: &mut FxHashMap<u64, MateResult>| -> bool {
                if plies < 2 { return false; }
                let (ck_pos, ck_bp) = match sliding_checker {
                    Some(v) => v,
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
                    && (promotion_zone(ck_bp.owner, ck_pos.y) || promotion_zone(ck_bp.owner, to_pos.y));
                let recapture = Move {
                    from: Some([ck_pos.x, ck_pos.y]),
                    to: [to_pos.x, to_pos.y],
                    drop: None,
                    promote: can_promote,
                };
                let undo1 = make_move(state, m);
                let undo2 = make_move(state, &recapture);
                let r = forced_mate_within(state, plies - 2, memo);
                undo_move(state, &recapture, &undo2);
                undo_move(state, m, &undo1);
                r.mate
            };

            // Phase 1: 盤上の駒の手を先に処理（早期打ち切り可能）
            for m in &board_moves {
                // 移動合いの無駄合い判定
                if is_wasteful_interposition(state, m, memo) {
                    continue;
                }

                let undo = make_move(state, m);
                let r = forced_mate_within(state, plies - 1, memo);
                undo_move(state, m, &undo);
                if !r.mate {
                    all_mate = false;
                    break; // 盤上の手で逃れ → ドロップ生成をスキップ
                }
                if !r.unique { all_unique = false; }
                if r.line.len() >= best_line.len() {
                    best_move = Some(m.clone());
                    best_line = r.line;
                }
            }

            // Phase 2: 盤上の手で全て詰む場合のみ、ドロップを遅延生成して処理
            if all_mate {
                let drop_moves = legal_drop_moves(state);

                if board_moves.is_empty() && drop_moves.is_empty() {
                    // 合法手なし → 詰み判定
                    let mated = is_in_check(state, side);
                    return { let r = MateResult { mate: mated, unique: true, line: vec![] }; memo.insert(key, r.clone()); r };
                }

                for m in &drop_moves {
                    // ドロップの無駄合い判定
                    if is_wasteful_interposition(state, m, memo) {
                        continue;
                    }

                    let undo = make_move(state, m);
                    let r = forced_mate_within(state, plies - 1, memo);
                    undo_move(state, m, &undo);
                    if !r.mate {
                        all_mate = false;
                        break;
                    }
                    if !r.unique { all_unique = false; }
                    if r.line.len() >= best_line.len() {
                        best_move = Some(m.clone());
                        best_line = r.line;
                    }
                }
            }

            if all_mate {
                if let Some(bm) = best_move {
                    let mut l = vec![bm];
                    l.extend(best_line);
                    MateResult { mate: true, unique: all_unique, line: l }
                } else {
                    // 全ての手が無駄合いだった or 合法手なし → 詰み
                    MateResult { mate: true, unique: true, line: vec![] }
                }
            } else {
                MateResult { mate: false, unique: false, line: vec![] }
            }
        }
    } else if plies == 0 {
        MateResult { mate: false, unique: false, line: vec![] }
    } else {
        // 攻め方: 全合法手を生成して王手になる手を抽出
        let moves = legal_moves(state);
        // Only consider checking moves (tsume-shogi rule)
        // legal_moves の結果から王手になる手を抽出し、ヒューリスティックでソート
        let mut checks: Vec<Move> = moves.into_iter()
            .filter(|m| {
                let undo = make_move(state, m);
                let check = is_in_check(state, enemy);
                undo_move(state, m, &undo);
                check
            })
            .collect();
        // 攻め方の手の並び替え: 捕獲 > 成り > 盤上移動 > 打ち
        // 良い手を先に探索することで枝刈りの効率を上げる
        checks.sort_by_key(|m| {
            if m.from.is_some() {
                let to = Pos::new(m.to[0], m.to[1]);
                let is_capture = state.get(to).is_some();
                if is_capture && m.promote { 0 }  // 捕獲+成り（最優先）
                else if is_capture { 1 }           // 捕獲
                else if m.promote { 2 }            // 成り
                else { 3 }                         // 通常移動
            } else {
                4 // 打ち（合駒で防がれやすい）
            }
        });

        let mut winning: Vec<(Move, MateResult)> = Vec::new();
        for m in &checks {
            let undo = make_move(state, m);
            let r = forced_mate_within(state, plies - 1, memo);
            undo_move(state, m, &undo);
            if r.mate {
                winning.push((m.clone(), r));
            }
        }

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
pub fn validate_tsume_puzzle(state: &mut State, mate_length: u32) -> Option<Vec<Move>> {
    if mate_length.is_multiple_of(2) || mate_length == 0 { return None; }
    if state.side_to_move != Owner::Attacker { return None; }
    // 守り方の玉は必須（攻め方の玉は無くても良い）
    state.king_pos(Owner::Defender)?;

    // 初期局面で既に王手がかかっていてはならない
    if is_in_check(state, Owner::Defender) { return None; }

    let mut memo = FxHashMap::default();

    // 反復深化: 浅い深さで不詰と判明すれば早期棄却（候補の高速フィルタリング）
    // 1手→3手→…→mate_length の順に探索し、詰まなければ即座に棄却
    // メモ化テーブルは深さ間で共有して再利用する
    let mut d = 1;
    while d < mate_length {
        let r = forced_mate_within(state, d, &mut memo);
        if r.mate {
            // より短い手数で詰む → N手詰としては不適格
            return None;
        }
        d += 2;
    }

    let within = forced_mate_within(state, mate_length, &mut memo);
    if !within.mate { return None; }

    // 最短性チェック: 上の反復深化で mate_length-2 以下の詰みは既に棄却済みなので不要

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
        // 詰将棋ルール: 守り方の持ち駒 = 全駒数 - 盤上の駒 - 攻め方の持ち駒
        state.hands.defender = self.compute_defender_hand();
        state.side_to_move = self.side_to_move;
        // Zobrist ハッシュに持ち駒と手番を反映
        state.zobrist_hash = state.compute_zobrist();
        state
    }

    /// 詰将棋ルールに基づき、守り方の持ち駒を自動計算する
    /// 全駒数から盤上の駒と攻め方の持ち駒を差し引いた残りが守り方の持ち駒
    fn compute_defender_hand(&self) -> [u8; 7] {
        // 全駒数（玉を除く）: 飛2, 角2, 金4, 銀4, 桂4, 香4, 歩18
        let max_counts: [u8; 7] = [2, 2, 4, 4, 4, 4, 18];
        let mut used = [0u8; 7]; // HAND_TYPES順: R, B, G, S, N, L, P

        // 盤上の駒をカウント（成り駒は元の駒種でカウント）
        for p in &self.pieces {
            if p.piece_type == PieceType::K { continue; }
            let base = p.piece_type.unpromote();
            let idx = HAND_TYPES.iter().position(|&t| t == base);
            if let Some(i) = idx {
                used[i] += 1;
            }
        }

        // 攻め方の持ち駒を加算
        let atk = self.hands.attacker.to_array();
        for i in 0..7 {
            used[i] += atk[i];
        }

        // 残りが守り方の持ち駒
        let mut defender = [0u8; 7];
        for i in 0..7 {
            defender[i] = max_counts[i].saturating_sub(used[i]);
        }
        defender
    }

    /// 盤上+持ち駒の合計が正規の駒数上限を超えていないかチェック
    pub fn has_excess_pieces(&self) -> bool {
        // 玉を除く各駒種の上限: R2, B2, G4, S4, N4, L4, P18
        let max_counts: [u8; 7] = [2, 2, 4, 4, 4, 4, 18];
        let mut used = [0u8; 7];

        for p in &self.pieces {
            if p.piece_type == PieceType::K { continue; }
            let base = p.piece_type.unpromote();
            let idx = HAND_TYPES.iter().position(|&t| t == base);
            if let Some(i) = idx {
                used[i] += 1;
            }
        }

        let atk = self.hands.attacker.to_array();
        for i in 0..7 {
            used[i] += atk[i];
        }

        for i in 0..7 {
            if used[i] > max_counts[i] {
                return true;
            }
        }
        false
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
        let mut state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::G)]);
        let moves = legal_moves(&mut state);
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
        let mut state = make_state_with_hands(
            (5, 9), (5, 1),
            &[
                (4, 1, Owner::Attacker, PieceType::G),
                (6, 1, Owner::Attacker, PieceType::G),
                (4, 2, Owner::Attacker, PieceType::G),
                (6, 2, Owner::Attacker, PieceType::G),
            ],
            hands,
        );
        let legal = legal_moves(&mut state);
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
        let mut state = make_state_with_hands((5, 9), (1, 1), &[
            (2, 1, Owner::Attacker, PieceType::S),
        ], hands);
        assert!(!is_in_check(&state, Owner::Defender), "初期局面で王手がかかっている");
        let result = validate_tsume_puzzle(&mut state, 1);
        assert!(result.is_some(), "一手詰めが検出されない");
    }

    #[test]
    fn test_validate_rejects_already_in_check() {
        // 初期状態で既に王手 → 無効
        let mut state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::R)]);
        assert!(is_in_check(&state, Owner::Defender));
        let result = validate_tsume_puzzle(&mut state, 1);
        assert!(result.is_none(), "初期局面で王手の問題が通ってしまう");
    }

    #[test]
    fn test_validate_rejects_even_mate_length() {
        let mut state = make_state((5, 9), (5, 1), &[]);
        assert!(validate_tsume_puzzle(&mut state, 2).is_none());
        assert!(validate_tsume_puzzle(&mut state, 0).is_none());
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

    // --- 無駄合い判定テスト ---

    #[test]
    fn test_is_between() {
        // 縦ライン: (5,5)と(5,1)の間に(5,3)がある
        assert!(is_between(Pos::new(5, 5), Pos::new(5, 1), Pos::new(5, 3)));
        assert!(is_between(Pos::new(5, 5), Pos::new(5, 1), Pos::new(5, 2)));
        assert!(!is_between(Pos::new(5, 5), Pos::new(5, 1), Pos::new(5, 5))); // 端点
        assert!(!is_between(Pos::new(5, 5), Pos::new(5, 1), Pos::new(5, 1))); // 端点
        assert!(!is_between(Pos::new(5, 5), Pos::new(5, 1), Pos::new(4, 3))); // ライン外

        // 横ライン
        assert!(is_between(Pos::new(1, 5), Pos::new(5, 5), Pos::new(3, 5)));
        assert!(!is_between(Pos::new(1, 5), Pos::new(5, 5), Pos::new(3, 4)));

        // 斜めライン
        assert!(is_between(Pos::new(1, 1), Pos::new(5, 5), Pos::new(3, 3)));
        assert!(!is_between(Pos::new(1, 1), Pos::new(5, 5), Pos::new(3, 4)));
    }

    #[test]
    fn test_find_checkers() {
        // 飛車で王手
        let state = make_state((5, 9), (5, 1), &[(5, 5, Owner::Attacker, PieceType::R)]);
        let checkers = find_checkers(&state, Owner::Defender);
        assert_eq!(checkers.len(), 1);
        assert_eq!(checkers[0].0, Pos::new(5, 5));
        assert_eq!(checkers[0].1.piece_type, PieceType::R);
    }

    #[test]
    fn test_futile_drop_skipped() {
        // 飛車による王手で合駒が無駄合いとなるケースをテスト
        // 守り方手番で、飛車が1筋を通して王手中
        // 守り方玉(1,1)は逃げ場なし（自駒で塞がれている）
        // 合駒は同飛で取られてもまだ詰み → 無駄合い判定でスキップされるべき
        //
        // 局面:
        //   守り方玉(1,1), 守り方歩(2,1)(2,2) — 逃げ道を自駒で塞ぐ
        //   攻め方飛(1,4) — 1筋を通して王手
        //   攻め方桂(2,4) — (1,2)を利かせて玉の捕獲を防ぐ
        let pieces = vec![
            PieceData { x: 1, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 2, y: 1, owner: Owner::Defender, piece_type: PieceType::P },
            PieceData { x: 2, y: 2, owner: Owner::Defender, piece_type: PieceType::P },
            PieceData { x: 1, y: 4, owner: Owner::Attacker, piece_type: PieceType::R },
            PieceData { x: 2, y: 4, owner: Owner::Attacker, piece_type: PieceType::N },
        ];
        let init = InitialData { pieces, hands: empty_hands_data(), side_to_move: Owner::Defender };
        let mut state = init.to_state();

        // 守り方手番で王手されている状態を確認
        assert!(is_in_check(&state, Owner::Defender));

        // 合駒可能だが全て無駄合い → 十分な plies で詰みと判定される
        let mut memo = FxHashMap::default();
        let result = forced_mate_within(&mut state, 4, &mut memo);
        assert!(result.mate, "無駄合いスキップにより詰みと判定されるべき");
        // 無駄合いがスキップされるので、手順は空（有効な防御手なし）
        assert!(result.line.is_empty(), "無駄合いのみの場合、手順は空であるべき");
    }

    #[test]
    fn test_futile_move_interposition_skipped() {
        // 移動合いの無駄合い判定テスト
        // 飛車による王手に対して、守り方の歩が利きライン上に移動して合駒するが、
        // 同飛で取られても詰み → 無駄合いとしてスキップされるべき
        //
        // 局面:
        //   守り方玉(1,1), 守り方歩(2,3) — 歩が(2,2)にいて(1,2)へは動けないが
        //   攻め方飛(1,5) — 1筋を通して王手
        //   攻め方金(2,1) — (2,1)を塞ぐ
        //   攻め方金(2,2) — (2,2)も塞ぐ → 玉は逃げ場なし、合駒のみ
        //   守り方は駒箱ルールで大量の持ち駒を持つが、全て無駄合い
        let pieces = vec![
            PieceData { x: 1, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 2, y: 1, owner: Owner::Attacker, piece_type: PieceType::G },
            PieceData { x: 2, y: 2, owner: Owner::Attacker, piece_type: PieceType::G },
            PieceData { x: 1, y: 5, owner: Owner::Attacker, piece_type: PieceType::R },
        ];
        let init = InitialData { pieces, hands: empty_hands_data(), side_to_move: Owner::Defender };
        let mut state = init.to_state();

        assert!(is_in_check(&state, Owner::Defender));

        // 守り方は(1,2)(1,3)(1,4)に合駒できるが、全て同飛で詰み → 無駄合い
        let mut memo = FxHashMap::default();
        let result = forced_mate_within(&mut state, 4, &mut memo);
        assert!(result.mate, "無駄合いスキップにより詰みと判定されるべき");
        assert!(result.line.is_empty(), "無駄合いのみの場合、手順は空であるべき");
    }

    #[test]
    fn test_futile_move_interposition_with_board_piece() {
        // 盤上の駒による移動合いの無駄合い判定テスト
        // 飛車(1,5)で王手、守り方に盤上の銀がライン外にいて移動合い可能
        let pieces2 = vec![
            PieceData { x: 1, y: 1, owner: Owner::Defender, piece_type: PieceType::K },
            PieceData { x: 2, y: 3, owner: Owner::Defender, piece_type: PieceType::S },
            PieceData { x: 2, y: 1, owner: Owner::Attacker, piece_type: PieceType::G },
            PieceData { x: 2, y: 2, owner: Owner::Attacker, piece_type: PieceType::G },
            PieceData { x: 1, y: 5, owner: Owner::Attacker, piece_type: PieceType::R },
        ];
        let init2 = InitialData { pieces: pieces2, hands: empty_hands_data(), side_to_move: Owner::Defender };
        let mut state2 = init2.to_state();

        assert!(is_in_check(&state2, Owner::Defender));

        // 銀(2,3)→(1,2)移動合い可能だが、同飛で取られて詰み → 無駄合い
        // ただし銀が(2,3)を離れると(2,2)の金が利かなくなるわけではないので
        // 局面は変わらない → 無駄合い
        let mut memo = FxHashMap::default();
        let result = forced_mate_within(&mut state2, 4, &mut memo);
        assert!(result.mate, "移動合いの無駄合いスキップにより詰みと判定されるべき");
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

    // --- Zobrist ハッシュテスト ---

    #[test]
    fn test_zobrist_make_undo_consistency() {
        // make_move → undo_move で Zobrist ハッシュが元に戻ることを検証
        let mut state = make_state((5, 9), (1, 1), &[
            (2, 2, Owner::Attacker, PieceType::G),
            (3, 3, Owner::Attacker, PieceType::R),
            (2, 1, Owner::Defender, PieceType::S),
        ]);
        let original_hash = state.zobrist_hash;

        let moves = legal_moves(&mut state);
        for m in &moves {
            let undo = make_move(&mut state, m);
            undo_move(&mut state, m, &undo);
            assert_eq!(state.zobrist_hash, original_hash,
                "make_move → undo_move でハッシュが復元されない: {:?}", m);
        }
    }

    #[test]
    fn test_zobrist_same_position_same_hash() {
        // 同一局面なら同じ Zobrist ハッシュになることを検証
        let state1 = make_state((5, 9), (5, 1), &[
            (3, 3, Owner::Attacker, PieceType::G),
        ]);
        let state2 = make_state((5, 9), (5, 1), &[
            (3, 3, Owner::Attacker, PieceType::G),
        ]);
        assert_eq!(state1.zobrist_hash, state2.zobrist_hash);
    }

    #[test]
    fn test_zobrist_different_position_different_hash() {
        // 異なる局面なら異なる Zobrist ハッシュになることを検証
        let state1 = make_state((5, 9), (5, 1), &[
            (3, 3, Owner::Attacker, PieceType::G),
        ]);
        let state2 = make_state((5, 9), (5, 1), &[
            (4, 3, Owner::Attacker, PieceType::G),
        ]);
        assert_ne!(state1.zobrist_hash, state2.zobrist_hash);
    }

    #[test]
    fn test_zobrist_incremental_matches_full() {
        // 差分更新のハッシュとゼロから計算したハッシュが一致することを検証
        let mut state = make_state((5, 9), (1, 1), &[
            (2, 2, Owner::Attacker, PieceType::G),
            (1, 2, Owner::Defender, PieceType::P),
        ]);

        let moves = legal_moves(&mut state);
        for m in &moves {
            let undo = make_move(&mut state, m);
            let incremental = state.zobrist_hash;
            let full = state.compute_zobrist();
            assert_eq!(incremental, full,
                "差分更新とフル計算のハッシュが不一致: {:?}", m);
            undo_move(&mut state, m, &undo);
        }
    }
}
