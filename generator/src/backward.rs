//! 逆算法（バックワード方式）による詰将棋候補生成
//!
//! 詰み局面から手を巻き戻して初期局面を構築する。
//! ランダム法よりも長手数の詰将棋を効率的に生成できる。

use std::collections::HashSet;

use crate::shogi::*;

/// 簡易乱数生成器（generate.rs と同じ xorshift）
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

    fn ri(&mut self, min: i8, max: i8) -> i8 {
        let range = (max - min + 1) as u64;
        (self.next() % range) as i8 + min
    }

    fn pick<'a, T>(&mut self, arr: &'a [T]) -> &'a T {
        let idx = self.next() as usize % arr.len();
        &arr[idx]
    }

    fn next_f64(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
    }

    fn shuffle<T>(&mut self, arr: &mut [T]) {
        for i in (1..arr.len()).rev() {
            let j = self.next() as usize % (i + 1);
            arr.swap(i, j);
        }
    }
}

/// ステップ移動方向（owner変換前）
fn step_moves_raw(t: PieceType) -> &'static [(i8, i8)] {
    match t {
        PieceType::K => &[(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)],
        PieceType::G | PieceType::PP | PieceType::PS | PieceType::PN | PieceType::PL =>
            &[(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(0,1)],
        PieceType::S => &[(-1,-1),(0,-1),(1,-1),(-1,1),(1,1)],
        PieceType::N => &[(-1,-2),(1,-2)],
        PieceType::P => &[(0,-1)],
        _ => &[],
    }
}

fn slide_dirs_raw(t: PieceType) -> &'static [(i8, i8)] {
    match t {
        PieceType::R | PieceType::PR => &[(0,-1),(1,0),(0,1),(-1,0)],
        PieceType::B | PieceType::PB => &[(1,-1),(1,1),(-1,1),(-1,-1)],
        PieceType::L => &[(0,-1)],
        _ => &[],
    }
}

fn extra_steps_raw(t: PieceType) -> &'static [(i8, i8)] {
    match t {
        PieceType::PR => &[(-1,-1),(1,-1),(1,1),(-1,1)],
        PieceType::PB => &[(0,-1),(1,0),(0,1),(-1,0)],
        _ => &[],
    }
}

fn transform_dir(owner: Owner, dx: i8, dy: i8) -> (i8, i8) {
    match owner {
        Owner::Attacker => (dx, dy),
        Owner::Defender => (-dx, -dy),
    }
}

fn promotion_zone(owner: Owner, y: i8) -> bool {
    match owner {
        Owner::Attacker => y <= 3,
        Owner::Defender => y >= 7,
    }
}

/// あるマスが指定 owner の駒から攻撃されているか
fn is_square_attacked_by(state: &State, target: Pos, attacker: Owner) -> bool {
    // ステップ駒チェック（逆方向からの到達）
    for &(dx, dy) in &[(-1i8,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] {
        let p = Pos::new(target.x + dx, target.y + dy);
        if !p.is_valid() { continue; }
        if let Some(bp) = state.get(p) {
            if bp.owner != attacker { continue; }
            // bp が target に到達できるか
            for &(sdx, sdy) in step_moves_raw(bp.piece_type) {
                let (tx, ty) = transform_dir(attacker, sdx, sdy);
                if p.x + tx == target.x && p.y + ty == target.y {
                    return true;
                }
            }
            for &(sdx, sdy) in extra_steps_raw(bp.piece_type) {
                let (tx, ty) = transform_dir(attacker, sdx, sdy);
                if p.x + tx == target.x && p.y + ty == target.y {
                    return true;
                }
            }
        }
    }
    // 桂馬チェック
    let knight_offsets: [(i8, i8); 2] = match attacker {
        Owner::Attacker => [(1, 2), (-1, 2)],
        Owner::Defender => [(1, -2), (-1, -2)],
    };
    for &(dx, dy) in &knight_offsets {
        let p = Pos::new(target.x + dx, target.y + dy);
        if !p.is_valid() { continue; }
        if let Some(bp) = state.get(p) {
            if bp.owner == attacker && bp.piece_type == PieceType::N {
                return true;
            }
        }
    }
    // スライド駒チェック
    let slide_checks: [(i8, i8); 8] = [
        (0,-1),(1,0),(0,1),(-1,0), // 縦横
        (1,-1),(1,1),(-1,1),(-1,-1), // 斜め
    ];
    for &(dx, dy) in &slide_checks {
        let mut x = target.x + dx;
        let mut y = target.y + dy;
        while Pos::new(x, y).is_valid() {
            if let Some(bp) = state.get(Pos::new(x, y)) {
                if bp.owner == attacker {
                    // この駒が target にスライドで到達できるか
                    for &(sdx, sdy) in slide_dirs_raw(bp.piece_type) {
                        let (tx, ty) = transform_dir(attacker, sdx, sdy);
                        if tx == -dx && ty == -dy {
                            return true;
                        }
                    }
                }
                break; // 遮断
            }
            x += dx;
            y += dy;
        }
    }
    false
}

/// 詰み局面（攻め方の手番で王手をかけた直後、守り方に合法手がない状態）を生成する
fn generate_mated_position(rng: &mut Rng) -> Option<State> {
    // 守り方の玉を端寄りに配置（詰ませやすい）
    let dk_x = *rng.pick(&[1i8, 1, 2, 2, 8, 8, 9, 9, 3, 7]);
    let dk_y = rng.ri(1, 2); // 1-2段目

    let mut state = State::new();
    let dk_pos = Pos::new(dk_x, dk_y);
    state.set(dk_pos, Some(BoardPiece { owner: Owner::Defender, piece_type: PieceType::K }));

    // 玉の周辺8マスを列挙
    let mut king_neighbors = Vec::new();
    for &(dx, dy) in &[(-1i8,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] {
        let p = Pos::new(dk_x + dx, dk_y + dy);
        if p.is_valid() {
            king_neighbors.push(p);
        }
    }

    // 使用済みの位置
    let mut used = HashSet::new();
    used.insert((dk_pos.x, dk_pos.y));

    // まず逃げ道を塞ぐ（王手駒は後で配置）
    let block_types = [
        PieceType::G, PieceType::S, PieceType::P,
        PieceType::G, PieceType::S,
    ];

    // 各逃げ道マスを処理
    let mut escape_squares: Vec<Pos> = Vec::new();
    for &sq in &king_neighbors {
        // 盤外（is_valid で弾かれた）のマスは逃げられないのでスキップ
        if rng.next_f64() < 0.5 {
            // 攻め方の駒で塞ぐ
            let bt = *rng.pick(&block_types);
            let ok = match bt {
                PieceType::P | PieceType::L => sq.y > 1,
                PieceType::N => sq.y > 2,
                _ => true,
            };
            if ok {
                state.set(sq, Some(BoardPiece { owner: Owner::Attacker, piece_type: bt }));
                used.insert((sq.x, sq.y));
                continue;
            }
        }
        if rng.next_f64() < 0.3 {
            // 守り方の駒で塞ぐ
            let dt = *rng.pick(&[PieceType::P, PieceType::S]);
            let ok = match dt {
                PieceType::P => sq.y < 9,
                _ => true,
            };
            if ok {
                state.set(sq, Some(BoardPiece { owner: Owner::Defender, piece_type: dt }));
                used.insert((sq.x, sq.y));
                continue;
            }
        }
        // まだ塞がれていないマス = 逃げ道候補
        escape_squares.push(sq);
    }

    // 王手をかける駒を配置（接触王手を優先 → 合駒不可で確実性が高い）
    let check_types = if rng.next_f64() < 0.7 {
        // 接触王手用の駒種
        &[PieceType::G, PieceType::S, PieceType::R, PieceType::B,
          PieceType::PR, PieceType::PB, PieceType::P][..]
    } else {
        &[PieceType::R, PieceType::B, PieceType::L, PieceType::N,
          PieceType::G, PieceType::S][..]
    };
    let check_type = *rng.pick(check_types);

    // 王手位置の候補を探す
    let mut check_candidates = Vec::new();

    // ステップ移動で王手できる位置
    for &(dx, dy) in step_moves_raw(check_type) {
        let (tx, ty) = transform_dir(Owner::Attacker, dx, dy);
        let pos = Pos::new(dk_x - tx, dk_y - ty);
        if pos.is_valid() && !used.contains(&(pos.x, pos.y)) {
            check_candidates.push(pos);
        }
    }
    for &(dx, dy) in extra_steps_raw(check_type) {
        let (tx, ty) = transform_dir(Owner::Attacker, dx, dy);
        let pos = Pos::new(dk_x - tx, dk_y - ty);
        if pos.is_valid() && !used.contains(&(pos.x, pos.y)) {
            check_candidates.push(pos);
        }
    }
    // スライドでの王手位置（近距離を優先）
    for &(dx, dy) in slide_dirs_raw(check_type) {
        let (tx, ty) = transform_dir(Owner::Attacker, dx, dy);
        let mut x = dk_x - tx;
        let mut y = dk_y - ty;
        let mut dist = 0;
        while Pos::new(x, y).is_valid() && dist < 3 {
            let p = Pos::new(x, y);
            if !used.contains(&(p.x, p.y)) {
                check_candidates.push(p);
            } else {
                break; // 遮断
            }
            x -= tx;
            y -= ty;
            dist += 1;
        }
    }
    // 桂馬の王手
    if check_type == PieceType::N {
        for &(dx, dy) in &[(1i8, 2), (-1, 2)] {
            let pos = Pos::new(dk_x + dx, dk_y + dy);
            if pos.is_valid() && !used.contains(&(pos.x, pos.y)) && pos.y > 2 {
                check_candidates.push(pos);
            }
        }
    }

    if check_candidates.is_empty() { return None; }
    rng.shuffle(&mut check_candidates);
    let check_pos = check_candidates[0];

    // 行き場のない駒の配置チェック
    match check_type {
        PieceType::P | PieceType::L => { if check_pos.y == 1 { return None; } }
        PieceType::N => { if check_pos.y <= 2 { return None; } }
        _ => {}
    }

    state.set(check_pos, Some(BoardPiece { owner: Owner::Attacker, piece_type: check_type }));
    used.insert((check_pos.x, check_pos.y));

    // 残りの逃げ道を王手駒の利きでカバーできるか確認し、
    // カバーできないマスには追加の駒を配置
    for &sq in &escape_squares {
        if used.contains(&(sq.x, sq.y)) { continue; }
        // 玉がこのマスに移動した場合、攻め方の利きがあるか
        let mut test = state.clone();
        test.set(dk_pos, None);
        test.set(sq, Some(BoardPiece { owner: Owner::Defender, piece_type: PieceType::K }));
        if !is_square_attacked_by(&test, sq, Owner::Attacker) {
            // 利きがない → 追加の駒で塞ぐ必要がある
            let bt = *rng.pick(&block_types);
            let ok = match bt {
                PieceType::P | PieceType::L => sq.y > 1,
                PieceType::N => sq.y > 2,
                _ => true,
            };
            if ok {
                // このマスに駒を置くのではなく、このマスに利く位置に駒を配置
                // 簡易的にこのマスに直接駒を配置する
                state.set(sq, Some(BoardPiece { owner: Owner::Attacker, piece_type: bt }));
                used.insert((sq.x, sq.y));
            }
        }
    }

    // 王手がかかっていることを確認
    if !is_in_check(&state, Owner::Defender) {
        return None;
    }

    // 守り方の手番にして合法手がないことを確認（詰み）
    state.side_to_move = Owner::Defender;
    if has_any_legal_move_simple(&state) {
        return None;
    }

    // 攻め方の手番に戻す
    state.side_to_move = Owner::Attacker;

    Some(state)
}

/// 合法手があるかの簡易判定（玉の移動のみ）
fn has_any_legal_move_simple(state: &State) -> bool {
    let owner = state.side_to_move;
    let kp = match state.king_pos(owner) {
        Some(p) => p,
        None => return false,
    };

    // 玉の移動
    for &(dx, dy) in &[(-1i8,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] {
        let np = Pos::new(kp.x + dx, kp.y + dy);
        if !np.is_valid() { continue; }
        if let Some(bp) = state.get(np) {
            if bp.owner == owner { continue; } // 自分の駒がある
        }
        // このマスが攻め方に利かれていないか
        // 一時的に玉を移動させて判定
        let mut test = state.clone();
        test.set(kp, None);
        test.set(np, Some(BoardPiece { owner, piece_type: PieceType::K }));
        if !is_square_attacked_by(&test, np, owner.opposite()) {
            return true;
        }
    }

    false
}

/// 攻め方の最終手を巻き戻す（1手巻き戻し）
/// 詰み局面から、攻め方の最終手を元に戻して「1手詰の初期局面」を作る
fn unwind_attacker_move(rng: &mut Rng, state: &State) -> Option<State> {
    // 王手をかけている駒を探す
    let dk_pos = state.king_pos(Owner::Defender)?;
    let checkers = find_checking_pieces(state, dk_pos);
    if checkers.is_empty() { return None; }

    // ランダムに1つ選ぶ
    let &(ck_pos, ck_piece) = rng.pick(&checkers);

    // この駒を元の位置に戻す（逆算）
    // パターン1: 盤上の別の位置から移動してきた → 元の位置に戻す
    // パターン2: 持ち駒から打った → 持ち駒に戻す

    if rng.next_f64() < 0.3 {
        // パターン2: 打ち駒として巻き戻す
        // ただし成り駒は打てないので、成り駒の場合はスキップ
        if ck_piece.piece_type.is_promoted() { return None; }
        // 打てる駒種のみ
        if !ck_piece.piece_type.is_hand_type() { return None; }

        let mut new_state = state.clone();
        new_state.set(ck_pos, None);
        new_state.hands.add(Owner::Attacker, ck_piece.piece_type, 1);
        new_state.side_to_move = Owner::Attacker;

        // 王手が外れていることを確認
        if is_in_check(&new_state, Owner::Defender) { return None; }

        Some(new_state)
    } else {
        // パターン1: 盤上移動として巻き戻す
        // 元の位置の候補を列挙（この駒種が ck_pos に到達できる位置）
        let mut from_candidates = Vec::new();
        let t = ck_piece.piece_type;
        let owner = Owner::Attacker;

        // ステップ移動の逆（to から from を逆算）
        // 駒が from にいて transform_dir(dx,dy) を加えると to に到達
        // → from = to - transform_dir(dx,dy)
        for &(dx, dy) in step_moves_raw(t) {
            let (tx, ty) = transform_dir(owner, dx, dy);
            let from = Pos::new(ck_pos.x - tx, ck_pos.y - ty);
            if from.is_valid() && from != dk_pos && state.get(from).is_none() {
                from_candidates.push((from, false)); // (元の位置, 成りフラグ)
            }
        }
        for &(dx, dy) in extra_steps_raw(t) {
            let (tx, ty) = transform_dir(owner, dx, dy);
            let from = Pos::new(ck_pos.x - tx, ck_pos.y - ty);
            if from.is_valid() && from != dk_pos && state.get(from).is_none() {
                from_candidates.push((from, false));
            }
        }
        // スライド移動の逆
        for &(dx, dy) in slide_dirs_raw(t) {
            let (tx, ty) = transform_dir(owner, dx, dy);
            let mut x = ck_pos.x - tx;
            let mut y = ck_pos.y - ty;
            while Pos::new(x, y).is_valid() {
                let p = Pos::new(x, y);
                if p == dk_pos { break; }
                if state.get(p).is_some() { break; }
                from_candidates.push((p, false));
                x -= tx;
                y -= ty;
            }
        }

        // 成りの逆算: 成った状態→元の駒に戻す
        if t.is_promoted() {
            let base = t.unpromote();
            // base の移動方向から ck_pos に到達できる from を探す
            for &(dx, dy) in step_moves_raw(base) {
                let (tx, ty) = transform_dir(owner, dx, dy);
                let from = Pos::new(ck_pos.x - tx, ck_pos.y - ty);
                if from.is_valid() && from != dk_pos && state.get(from).is_none() {
                    // 成り条件: from か to が敵陣
                    if promotion_zone(owner, from.y) || promotion_zone(owner, ck_pos.y) {
                        from_candidates.push((from, true)); // 成りを巻き戻す
                    }
                }
            }
            for &(dx, dy) in slide_dirs_raw(base) {
                let (tx, ty) = transform_dir(owner, dx, dy);
                let mut x = ck_pos.x - tx;
                let mut y = ck_pos.y - ty;
                while Pos::new(x, y).is_valid() {
                    let p = Pos::new(x, y);
                    if p == dk_pos { break; }
                    if state.get(p).is_some() { break; }
                    if promotion_zone(owner, p.y) || promotion_zone(owner, ck_pos.y) {
                        from_candidates.push((p, true));
                    }
                    x -= tx;
                    y -= ty;
                }
            }
        }

        if from_candidates.is_empty() { return None; }
        rng.shuffle(&mut from_candidates);
        let (from_pos, was_promotion) = from_candidates[0];

        let mut new_state = state.clone();
        new_state.set(ck_pos, None);
        let piece_type_at_from = if was_promotion {
            ck_piece.piece_type.unpromote()
        } else {
            ck_piece.piece_type
        };

        // 行き場のない駒チェック
        match piece_type_at_from {
            PieceType::P | PieceType::L => {
                if from_pos.y == 1 { return None; }
            }
            PieceType::N => {
                if from_pos.y <= 2 { return None; }
            }
            _ => {}
        }

        new_state.set(from_pos, Some(BoardPiece { owner, piece_type: piece_type_at_from }));
        new_state.side_to_move = Owner::Attacker;

        // 王手が外れていることを確認
        if is_in_check(&new_state, Owner::Defender) { return None; }

        Some(new_state)
    }
}

/// 守り方の手を巻き戻す（玉の移動の逆算）
///
/// 現在の局面は「攻め方が王手をかけた直後」の状態。
/// 守り方の巻き戻しでは:
/// パターンA: 玉を空きマスに移動（逃げる前の位置に戻す）
/// パターンB: 玉が攻め方の駒を取って逃げた（捕獲の逆算: 元の位置に駒を復元）
/// いずれも新しい玉位置に王手がかかっている状態を作る
///    → 結果: 「攻め方が別の王手をかけた直後」の状態になる
fn unwind_defender_move(rng: &mut Rng, state: &State) -> Option<State> {
    let dk_pos = state.king_pos(Owner::Defender)?;

    // パターンB: 玉が攻め方の駒を取って逃げた場合の逆算（30%の確率で先に試行）
    if rng.next_f64() < 0.3 {
        if let Some(s) = unwind_defender_capture(rng, state, dk_pos) {
            return Some(s);
        }
    }

    // パターンA: 玉を空きマスに移動（逃げる前の位置）
    let mut from_candidates = Vec::new();
    for &(dx, dy) in &[(-1i8,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] {
        let old_pos = Pos::new(dk_pos.x + dx, dk_pos.y + dy);
        if !old_pos.is_valid() { continue; }
        if state.get(old_pos).is_some() { continue; }
        from_candidates.push(old_pos);
    }

    if !from_candidates.is_empty() {
        rng.shuffle(&mut from_candidates);

        for &old_king_pos in &from_candidates {
            let mut new_state = state.clone();
            new_state.set(dk_pos, None);
            new_state.set(old_king_pos, Some(BoardPiece { owner: Owner::Defender, piece_type: PieceType::K }));
            new_state.side_to_move = Owner::Defender;

            // まず既存の駒で王手がかかっているか確認
            if is_in_check(&new_state, Owner::Defender) {
                return Some(new_state);
            }

            // 王手がかかっていない場合、攻め方の駒を追加して王手をかける
            if let Some(checked_state) = add_checking_piece(rng, &new_state, old_king_pos) {
                return Some(checked_state);
            }
        }
    }

    // パターンB をまだ試していなければ試す
    unwind_defender_capture(rng, state, dk_pos)
}

/// 守り方の玉が攻め方の駒を取って逃げた場合の逆算
/// 現在の玉位置に攻め方の駒を復元し、元の位置（隣接マス）に玉を戻す
fn unwind_defender_capture(rng: &mut Rng, state: &State, dk_pos: Pos) -> Option<State> {
    // 現在の玉位置に攻め方の駒があった（玉が取った）
    let capture_types = [
        PieceType::G, PieceType::S, PieceType::P, PieceType::N, PieceType::L,
        PieceType::R, PieceType::B,
    ];
    let captured_type = *rng.pick(&capture_types);

    // 元の玉位置（逃げる前）の候補
    let mut from_candidates = Vec::new();
    for &(dx, dy) in &[(-1i8,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] {
        let old_pos = Pos::new(dk_pos.x + dx, dk_pos.y + dy);
        if !old_pos.is_valid() { continue; }
        if state.get(old_pos).is_some() { continue; }
        from_candidates.push(old_pos);
    }

    if from_candidates.is_empty() { return None; }
    rng.shuffle(&mut from_candidates);

    for &old_king_pos in &from_candidates {
        let mut new_state = state.clone();
        // 現在の玉位置に攻め方の駒を復元（玉が取った駒）
        new_state.set(dk_pos, Some(BoardPiece { owner: Owner::Attacker, piece_type: captured_type }));
        // 元の位置に玉を配置
        new_state.set(old_king_pos, Some(BoardPiece { owner: Owner::Defender, piece_type: PieceType::K }));
        new_state.side_to_move = Owner::Defender;

        // 王手がかかっているか確認
        if is_in_check(&new_state, Owner::Defender) {
            return Some(new_state);
        }

        // 王手をかける駒を追加
        if let Some(checked_state) = add_checking_piece(rng, &new_state, old_king_pos) {
            return Some(checked_state);
        }
    }

    None
}

/// 指定位置の玉に王手をかける攻め方の駒を追加する
fn add_checking_piece(rng: &mut Rng, state: &State, king_pos: Pos) -> Option<State> {
    let check_types = [
        PieceType::G, PieceType::S, PieceType::R, PieceType::B,
        PieceType::N, PieceType::P, PieceType::L,
    ];

    let mut candidates: Vec<(Pos, PieceType)> = Vec::new();

    for &ct in &check_types {
        // ステップ移動で王手できる位置
        for &(dx, dy) in step_moves_raw(ct) {
            let (tx, ty) = transform_dir(Owner::Attacker, dx, dy);
            let from = Pos::new(king_pos.x - tx, king_pos.y - ty);
            if !from.is_valid() { continue; }
            if state.get(from).is_some() { continue; }
            // 行き場のない駒チェック
            let ok = match ct {
                PieceType::P | PieceType::L => from.y > 1,
                PieceType::N => from.y > 2,
                _ => true,
            };
            if ok {
                candidates.push((from, ct));
            }
        }
        // スライド移動で王手できる位置
        for &(dx, dy) in slide_dirs_raw(ct) {
            let (tx, ty) = transform_dir(Owner::Attacker, dx, dy);
            let mut x = king_pos.x - tx;
            let mut y = king_pos.y - ty;
            let mut dist = 0;
            while Pos::new(x, y).is_valid() && dist < 3 {
                let p = Pos::new(x, y);
                if state.get(p).is_some() { break; }
                // 行き場のない駒チェック
                let ok = match ct {
                    PieceType::L => p.y > 1,
                    _ => true,
                };
                if ok {
                    candidates.push((p, ct));
                }
                x -= tx;
                y -= ty;
                dist += 1;
            }
        }
    }

    // 桂馬の王手位置
    for &(dx, dy) in &[(1i8, 2), (-1, 2)] {
        let from = Pos::new(king_pos.x + dx, king_pos.y + dy);
        if !from.is_valid() { continue; }
        if state.get(from).is_some() { continue; }
        if from.y > 2 { // 攻め方の桂馬は3段目以上に配置
            candidates.push((from, PieceType::N));
        }
    }

    if candidates.is_empty() { return None; }
    rng.shuffle(&mut candidates);

    // 最初の有効な候補を使用
    let (pos, piece_type) = candidates[0];
    let mut new_state = state.clone();
    new_state.set(pos, Some(BoardPiece { owner: Owner::Attacker, piece_type }));

    // 確認: 実際に王手がかかっているか
    if !is_in_check(&new_state, Owner::Defender) {
        return None;
    }

    Some(new_state)
}

/// 王手をかけている駒を探す
fn find_checking_pieces(state: &State, king_pos: Pos) -> Vec<(Pos, BoardPiece)> {
    let attacker = Owner::Attacker;
    let mut checkers = Vec::new();

    // ステップ駒
    for &(dx, dy) in &[(-1i8,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] {
        let p = Pos::new(king_pos.x + dx, king_pos.y + dy);
        if !p.is_valid() { continue; }
        if let Some(bp) = state.get(p) {
            if bp.owner == attacker {
                // bp が king_pos を攻撃できるか
                for &(sdx, sdy) in step_moves_raw(bp.piece_type) {
                    let (tx, ty) = transform_dir(attacker, sdx, sdy);
                    if p.x + tx == king_pos.x && p.y + ty == king_pos.y {
                        checkers.push((p, bp));
                        break;
                    }
                }
                for &(sdx, sdy) in extra_steps_raw(bp.piece_type) {
                    let (tx, ty) = transform_dir(attacker, sdx, sdy);
                    if p.x + tx == king_pos.x && p.y + ty == king_pos.y {
                        if !checkers.iter().any(|&(cp, _)| cp == p) {
                            checkers.push((p, bp));
                        }
                        break;
                    }
                }
            }
        }
    }

    // 桂馬
    for &(dx, dy) in &[(1i8, 2), (-1, 2)] {
        let p = Pos::new(king_pos.x + dx, king_pos.y + dy);
        if !p.is_valid() { continue; }
        if let Some(bp) = state.get(p) {
            if bp.owner == attacker && bp.piece_type == PieceType::N {
                checkers.push((p, bp));
            }
        }
    }

    // スライド駒
    let slide_checks: [(i8, i8); 8] = [
        (0,-1),(1,0),(0,1),(-1,0),
        (1,-1),(1,1),(-1,1),(-1,-1),
    ];
    for &(dx, dy) in &slide_checks {
        let mut x = king_pos.x + dx;
        let mut y = king_pos.y + dy;
        while Pos::new(x, y).is_valid() {
            if let Some(bp) = state.get(Pos::new(x, y)) {
                if bp.owner == attacker {
                    for &(sdx, sdy) in slide_dirs_raw(bp.piece_type) {
                        let (tx, ty) = transform_dir(attacker, sdx, sdy);
                        if tx == -dx && ty == -dy {
                            checkers.push((Pos::new(x, y), bp));
                            break;
                        }
                    }
                }
                break;
            }
            x += dx;
            y += dy;
        }
    }

    checkers
}

/// 逆算法で1つの候補局面を生成する
///
/// 1. 詰み局面を生成
/// 2. 攻め方の最終手を巻き戻し（1手詰の元局面を作る）
/// 3. 3手詰以上: 守り方の手→攻め方の手を交互に巻き戻す
/// 4. 不要な駒を削除
pub fn backward_candidate(rng_seed: u64, mate_length: u32) -> Option<InitialData> {
    let mut rng = Rng::new(rng_seed);

    // ステップ1: 詰み局面を生成
    let mut mated = None;
    for _ in 0..20 {
        if let Some(s) = generate_mated_position(&mut rng) {
            mated = Some(s);
            break;
        }
    }
    let mated = mated?;

    // ステップ2: 攻め方の最終手を巻き戻し
    let mut current = None;
    for _ in 0..10 {
        if let Some(s) = unwind_attacker_move(&mut rng, &mated) {
            current = Some(s);
            break;
        }
    }
    let mut current = current?;

    // ステップ3: 3手詰以上の場合、守り方→攻め方の巻き戻しを繰り返す
    let additional_pairs = (mate_length.saturating_sub(1)) / 2;
    for _ in 0..additional_pairs {
        // 守り方の手を巻き戻す
        let mut unwound_def = None;
        for _ in 0..20 {
            if let Some(s) = unwind_defender_move(&mut rng, &current) {
                unwound_def = Some(s);
                break;
            }
        }
        current = unwound_def?;

        // 攻め方の手を巻き戻す
        let mut unwound_atk = None;
        for _ in 0..20 {
            if let Some(s) = unwind_attacker_move(&mut rng, &current) {
                unwound_atk = Some(s);
                break;
            }
        }
        current = unwound_atk?;
    }

    // side_to_move を攻め方にする
    // 駒の削減は validate_and_prune 側の prune_initial に任せる
    current.side_to_move = Owner::Attacker;
    let final_state = current;

    // InitialData に変換
    Some(InitialData::from_state(&final_state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mated_position() {
        // 複数のシードで詰み局面が生成できるか
        let mut found = 0;
        for seed in 0..100 {
            let mut rng = Rng::new(seed + 1);
            if generate_mated_position(&mut rng).is_some() {
                found += 1;
            }
        }
        assert!(found > 0, "100回のシードで詰み局面が1つも生成されなかった");
        eprintln!("詰み局面生成: {}/100 成功", found);
    }

    #[test]
    fn test_backward_candidate_1mate() {
        let mut found = 0;
        for seed in 0..200 {
            if backward_candidate(seed + 1, 1).is_some() {
                found += 1;
            }
        }
        assert!(found > 0, "200回のシードで1手詰候補が1つも生成されなかった");
        eprintln!("1手詰候補生成: {}/200 成功", found);
    }

    #[test]
    fn test_backward_candidate_3mate() {
        let mut found = 0;
        for seed in 0..500 {
            if backward_candidate(seed + 1, 3).is_some() {
                found += 1;
            }
        }
        assert!(found > 0, "500回のシードで3手詰候補が1つも生成されなかった");
        eprintln!("3手詰候補生成: {}/500 成功", found);
    }

    #[test]
    fn test_backward_candidate_produces_valid_board() {
        for seed in 0..100 {
            if let Some(init) = backward_candidate(seed + 1, 1) {
                // 基本的な検証
                let dk = init.pieces.iter().find(|p| p.owner == Owner::Defender && p.piece_type == PieceType::K);
                assert!(dk.is_some(), "守り方の玉がない");
                for p in &init.pieces {
                    assert!(p.x >= 1 && p.x <= 9, "x座標が範囲外: {}", p.x);
                    assert!(p.y >= 1 && p.y <= 9, "y座標が範囲外: {}", p.y);
                }
                // 重複位置チェック
                let mut pos_set = HashSet::new();
                for p in &init.pieces {
                    assert!(pos_set.insert((p.x, p.y)), "重複位置: ({}, {})", p.x, p.y);
                }
            }
        }
    }
}
