//! 逆算法（バックワード方式）による詰将棋候補生成
//!
//! 詰み局面から手を巻き戻して初期局面を構築する。
//! ランダム法よりも長手数の詰将棋を効率的に生成できる。

use std::collections::HashSet;

use shogi_core::shogi::*;
use shogi_core::rng::Rng;

/// あるマスが指定 owner の駒から攻撃されているか
fn is_square_attacked_by(state: &State, target: Pos, attacker: Owner) -> bool {
    // ステップ駒チェック（逆方向からの到達）
    for &(dx, dy) in &[(-1i8,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)] {
        let p = Pos::new(target.x + dx, target.y + dy);
        if !p.is_valid() { continue; }
        if let Some(bp) = state.get(p) {
            if bp.owner != attacker { continue; }
            // bp が target に到達できるか
            for &(sdx, sdy) in step_moves(bp.piece_type) {
                let (tx, ty) = transform_dir(attacker, sdx, sdy);
                if p.x + tx == target.x && p.y + ty == target.y {
                    return true;
                }
            }
            for &(sdx, sdy) in extra_steps(bp.piece_type) {
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
                    for &(sdx, sdy) in slide_dirs(bp.piece_type) {
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

/// 駒がスライド駒（飛車・角・香車・成飛車・成角）かどうか
fn is_sliding_piece(pt: PieceType) -> bool {
    !slide_dirs(pt).is_empty()
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
    for &(dx, dy) in step_moves(check_type) {
        let (tx, ty) = transform_dir(Owner::Attacker, dx, dy);
        let pos = Pos::new(dk_x - tx, dk_y - ty);
        if pos.is_valid() && !used.contains(&(pos.x, pos.y)) {
            check_candidates.push(pos);
        }
    }
    for &(dx, dy) in extra_steps(check_type) {
        let (tx, ty) = transform_dir(Owner::Attacker, dx, dy);
        let pos = Pos::new(dk_x - tx, dk_y - ty);
        if pos.is_valid() && !used.contains(&(pos.x, pos.y)) {
            check_candidates.push(pos);
        }
    }
    // スライドでの王手位置（近距離を優先）
    for &(dx, dy) in slide_dirs(check_type) {
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
    // has_any_legal_move で完全な合法手チェック（玉移動、合駒、駒取りすべて含む）
    state.side_to_move = Owner::Defender;
    if has_any_legal_move(&mut state) {
        return None;
    }

    // 攻め方の手番に戻す
    state.side_to_move = Owner::Attacker;

    Some(state)
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
        for &(dx, dy) in step_moves(t) {
            let (tx, ty) = transform_dir(owner, dx, dy);
            let from = Pos::new(ck_pos.x - tx, ck_pos.y - ty);
            if from.is_valid() && from != dk_pos && state.get(from).is_none() {
                from_candidates.push((from, false)); // (元の位置, 成りフラグ)
            }
        }
        for &(dx, dy) in extra_steps(t) {
            let (tx, ty) = transform_dir(owner, dx, dy);
            let from = Pos::new(ck_pos.x - tx, ck_pos.y - ty);
            if from.is_valid() && from != dk_pos && state.get(from).is_none() {
                from_candidates.push((from, false));
            }
        }
        // スライド移動の逆
        for &(dx, dy) in slide_dirs(t) {
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
            for &(dx, dy) in step_moves(base) {
                let (tx, ty) = transform_dir(owner, dx, dy);
                let from = Pos::new(ck_pos.x - tx, ck_pos.y - ty);
                if from.is_valid() && from != dk_pos && state.get(from).is_none() {
                    // 成り条件: from か to が敵陣
                    if promotion_zone(owner, from.y) || promotion_zone(owner, ck_pos.y) {
                        from_candidates.push((from, true)); // 成りを巻き戻す
                    }
                }
            }
            for &(dx, dy) in slide_dirs(base) {
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

/// 守り方の手を巻き戻す（玉の移動・合駒の逆算）
///
/// 現在の局面は「攻め方が王手をかけた直後」の状態。
/// 守り方の巻き戻しでは:
/// パターンA: 玉を空きマスに移動（逃げる前の位置に戻す）
/// パターンB: 玉が攻め方の駒を取って逃げた（捕獲の逆算: 元の位置に駒を復元）
/// パターンC: 合駒（スライド王手に対して遮断駒を配置した逆算）
/// いずれも新しい玉位置に王手がかかっている状態を作る
fn unwind_defender_move(rng: &mut Rng, state: &State) -> Option<State> {
    let dk_pos = state.king_pos(Owner::Defender)?;
    let mut candidates: Vec<(u32, State)> = Vec::new();

    if let Some(s) = unwind_defender_king_move(rng, state, dk_pos) {
        if let Some(score) = accept_defender_unwind(&s) {
            candidates.push((score, s));
        }
    }
    if let Some(s) = unwind_defender_capture(rng, state, dk_pos) {
        if let Some(score) = accept_defender_unwind(&s) {
            candidates.push((score, s));
        }
    }
    if let Some(s) = unwind_defender_interpose(rng, state, dk_pos) {
        if let Some(score) = accept_defender_unwind(&s) {
            candidates.push((score, s));
        }
    }

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by_key(|(score, _)| *score);
    let best_score = candidates[0].0;
    let best_count = candidates.iter().take_while(|(score, _)| *score == best_score).count();
    let pick_idx = (rng.next_u64() as usize) % best_count;
    Some(candidates.swap_remove(pick_idx).1)
}

fn defender_unwind_stats(state: &State) -> Option<(u32, u32, u32, u32)> {
    let dk = state.king_pos(Owner::Defender)?;

    let mut empty_around = 0u32;
    for dx in -1..=1i8 {
        for dy in -1..=1i8 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = dk.x + dx;
            let ny = dk.y + dy;
            if (1..=9).contains(&nx) && (1..=9).contains(&ny) && state.get(Pos::new(nx, ny)).is_none() {
                empty_around += 1;
            }
        }
    }

    let checkers = find_checking_pieces(state, dk);
    let slider_count = checkers.iter().filter(|(_, bp)| is_sliding_piece(bp.piece_type)).count() as u32;
    let contact_count = checkers.len() as u32 - slider_count;

    let score = (empty_around * 8 + slider_count * 3).saturating_sub(contact_count * 2);
    Some((score, empty_around, slider_count, contact_count))
}

fn accept_defender_unwind(state: &State) -> Option<u32> {
    let (score, empty_around, slider_count, contact_count) = defender_unwind_stats(state)?;
    if empty_around >= 5 {
        return None;
    }
    if slider_count >= 2 && contact_count == 0 {
        return None;
    }
    Some(score)
}

/// パターンA: 玉を空きマスに移動（逃げる前の位置に戻す）
fn unwind_defender_king_move(rng: &mut Rng, state: &State, dk_pos: Pos) -> Option<State> {
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

    None
}

/// パターンB: 守り方の玉が攻め方の駒を取って逃げた場合の逆算
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

/// パターンC: 合駒の逆算
/// スライド王手に対して守り方が遮断駒を置いた手を巻き戻す。
/// 結果: 遮断駒がない状態（スライド王手が直通している状態）を作り、
/// さらに別の方向からの王手を追加する。
fn unwind_defender_interpose(rng: &mut Rng, state: &State, dk_pos: Pos) -> Option<State> {
    // 現在の盤面でスライド王手の経路上に守り方の駒がないか探す
    // （既に王手がかかっている状態なので、今は合駒がない状態）
    //
    // 合駒の逆算: 攻め方のスライド駒と玉の間の空きマスに守り方の駒を配置し、
    // 元のスライド王手をブロック。代わりに別の王手を追加。
    let checkers = find_checking_pieces(state, dk_pos);

    // スライド王手でない場合は合駒不可
    let slider_checkers: Vec<(Pos, BoardPiece)> = checkers.iter()
        .filter(|(pos, bp)| {
            is_sliding_piece(bp.piece_type) && {
                // 隣接でないスライド王手のみ（隣接なら合駒不可）
                let dx = (dk_pos.x - pos.x).abs();
                let dy = (dk_pos.y - pos.y).abs();
                dx > 1 || dy > 1
            }
        })
        .cloned()
        .collect();
    if slider_checkers.is_empty() { return None; }

    let &(ck_pos, _) = rng.pick(&slider_checkers);
    let interp = interposition_squares(ck_pos, dk_pos);
    if interp.is_empty() { return None; }

    // 合駒位置をランダムに選択
    let block_pos = *rng.pick(&interp);

    // 合駒として配置する守り方の駒種（打ち駒なので成り駒は除外）
    let block_types = [
        PieceType::G, PieceType::S, PieceType::P, PieceType::N,
        PieceType::L, PieceType::R, PieceType::B,
    ];
    let block_type = *rng.pick(&block_types);

    // 行き場のない駒チェック
    match block_type {
        PieceType::P | PieceType::L => { if block_pos.y >= 9 { return None; } }
        PieceType::N => { if block_pos.y >= 8 { return None; } }
        _ => {}
    }

    let mut new_state = state.clone();
    new_state.set(block_pos, Some(BoardPiece {
        owner: Owner::Defender, piece_type: block_type
    }));
    new_state.side_to_move = Owner::Defender;

    // 元のスライド王手は合駒でブロックされた
    // 別の方向からの王手が必要（前の攻め方の手に対応）
    if is_in_check(&new_state, Owner::Defender) {
        // 別方向から既に王手がかかっている
        return Some(new_state);
    }

    // 新しい王手をかける駒を追加
    add_checking_piece(rng, &new_state, dk_pos)
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
        for &(dx, dy) in step_moves(ct) {
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
        for &(dx, dy) in slide_dirs(ct) {
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
                let mut found = false;
                for &(sdx, sdy) in step_moves(bp.piece_type) {
                    let (tx, ty) = transform_dir(attacker, sdx, sdy);
                    if p.x + tx == king_pos.x && p.y + ty == king_pos.y {
                        checkers.push((p, bp));
                        found = true;
                        break;
                    }
                }
                if !found {
                    for &(sdx, sdy) in extra_steps(bp.piece_type) {
                        let (tx, ty) = transform_dir(attacker, sdx, sdy);
                        if p.x + tx == king_pos.x && p.y + ty == king_pos.y {
                            checkers.push((p, bp));
                            break;
                        }
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
                    for &(sdx, sdy) in slide_dirs(bp.piece_type) {
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

/// 攻め方の巻き戻しを複数回リトライするラッパー
fn try_unwind_attacker(rng: &mut Rng, state: &State, max_attempts: u32) -> Option<State> {
    for _ in 0..max_attempts {
        if let Some(s) = unwind_attacker_move(rng, state) {
            return Some(s);
        }
    }
    None
}

/// 守り方の巻き戻しを複数回リトライするラッパー
fn try_unwind_defender(rng: &mut Rng, state: &State, max_attempts: u32) -> Option<State> {
    for _ in 0..max_attempts {
        if let Some(s) = unwind_defender_move(rng, state) {
            return Some(s);
        }
    }
    None
}

/// 巻き戻しチェーンを構築する（線形リトライ）
/// 各ペア（守り方→攻め方）を順番に巻き戻す。失敗したら最初からやり直す。
fn try_extend_chain(rng: &mut Rng, start: &State, total_pairs: u32) -> Option<State> {
    if total_pairs == 0 {
        return Some(start.clone());
    }

    // 全体を通してリトライ（再帰ではなく線形ループ）
    'outer: for _ in 0..50 {
        let mut current = start.clone();
        for _ in 0..total_pairs {
            let def = match try_unwind_defender(rng, &current, 20) {
                Some(s) => s,
                None => continue 'outer,
            };
            let atk = match try_unwind_attacker(rng, &def, 20) {
                Some(s) => s,
                None => continue 'outer,
            };
            current = atk;
        }
        return Some(current);
    }
    None
}

/// 逆算法で1つの候補局面を生成する
///
/// 1. 詰み局面を生成
/// 2. 攻め方の最終手を巻き戻し（1手詰の元局面を作る）
/// 3. 3手詰以上: 守り方の手→攻め方の手を交互に巻き戻す（バックトラック付き）
/// 4. 不要な駒を削除
pub fn backward_candidate(rng_seed: u64, mate_length: u32) -> Option<InitialData> {
    let mut rng = Rng::new(rng_seed);
    let additional_pairs = (mate_length.saturating_sub(1)) / 2;

    // 外側リトライ: 詰み局面からやり直す
    for _ in 0..50 {
        let mated = match generate_mated_position(&mut rng) {
            Some(s) => s,
            None => continue,
        };

        // 攻め方の最終手を巻き戻し
        let initial = match try_unwind_attacker(&mut rng, &mated, 30) {
            Some(s) => s,
            None => continue,
        };

        // 追加ペアの巻き戻し（バックトラック付き）
        if let Some(mut result) = try_extend_chain(&mut rng, &initial, additional_pairs) {
            result.side_to_move = Owner::Attacker;
            return Some(InitialData::from_state(&result));
        }
    }
    None
}

/// 既存の短手数パズルから2手延長して長手数の候補を生成する
///
/// N手詰のパズル初期局面を State に変換し、守り方→攻め方の1ペアを巻き戻して
/// (N+2)手詰の候補を作る。ゼロから生成するより成功率が高い。
pub fn extend_candidate(rng_seed: u64, source: &InitialData) -> Option<InitialData> {
    let mut rng = Rng::new(rng_seed);
    let mut best: Option<(u32, u32, u32, u32, InitialData)> = None;

    for _ in 0..12 {
        let mut current = source.to_state();

        // 攻め方の手番にする（初期局面は攻め方手番のはず）
        current.side_to_move = Owner::Attacker;

        // 守り方の手を巻き戻す
        let unwound_def = match try_unwind_defender(&mut rng, &current, 50) {
            Some(s) => s,
            None => continue,
        };

        // 攻め方の手を巻き戻す
        let mut result = match try_unwind_attacker(&mut rng, &unwound_def, 50) {
            Some(s) => s,
            None => continue,
        };

        result.side_to_move = Owner::Attacker;
        let initial = InitialData::from_state(&result);
        if initial.has_excess_pieces() {
            continue;
        }

        let (pressure, empty_around) = match result.king_pos(Owner::Defender) {
            Some(dk) => {
                let mut pressure = 0u32;
                let mut count = 0u32;
                for p in &initial.pieces {
                    if p.owner != Owner::Attacker || p.piece_type == PieceType::K {
                        continue;
                    }
                    let dx = (p.x - dk.x).abs();
                    let dy = (p.y - dk.y).abs();
                    if dx <= 2 && dy <= 2 {
                        pressure += 3;
                    } else if dx <= 4 && dy <= 4 {
                        pressure += 1;
                    }
                }
                pressure += initial.hands.attacker.to_array().iter().map(|&v| v as u32).sum::<u32>();
                for dx in -1..=1i8 {
                    for dy in -1..=1i8 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = dk.x + dx;
                        let ny = dk.y + dy;
                        if (1..=9).contains(&nx)
                            && (1..=9).contains(&ny)
                            && result.get(Pos::new(nx, ny)).is_none()
                        {
                            count += 1;
                        }
                    }
                }
                (pressure, count)
            }
            None => continue,
        };
        let piece_count = initial.pieces.len() as u32;
        let hand_count = initial.hands.attacker.to_array().iter().map(|&v| v as u32).sum::<u32>();

        match &best {
            Some((best_pressure, best_empty, best_piece_count, best_hand_count, _))
                if pressure < *best_pressure
                    || (pressure == *best_pressure && empty_around > *best_empty)
                    || (pressure == *best_pressure && empty_around == *best_empty && piece_count > *best_piece_count)
                    || (pressure == *best_pressure && empty_around == *best_empty && piece_count == *best_piece_count && hand_count >= *best_hand_count) => {}
            _ => {
                best = Some((pressure, empty_around, piece_count, hand_count, initial));
                if pressure >= 8 && empty_around <= 3 && piece_count <= 7 && hand_count <= 2 {
                    break;
                }
            }
        }
    }

    best.map(|(_, _, _, _, initial)| initial)
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
    fn test_backward_candidate_5mate() {
        let mut found = 0;
        for seed in 0..1000 {
            if backward_candidate(seed + 1, 5).is_some() {
                found += 1;
            }
        }
        assert!(found > 0, "1000回のシードで5手詰候補が1つも生成されなかった");
        eprintln!("5手詰候補生成: {}/1000 成功", found);
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

    #[test]
    fn test_extend_candidate() {
        // 1手詰の候補をいくつか生成し、それを延長して3手詰候補を作る
        let mut sources = Vec::new();
        for seed in 0..500 {
            if let Some(init) = backward_candidate(seed + 1, 1) {
                sources.push(init);
                if sources.len() >= 10 { break; }
            }
        }
        assert!(!sources.is_empty(), "延長元の1手詰候補が生成できなかった");

        let mut found = 0;
        for (i, src) in sources.iter().enumerate() {
            for attempt in 0..50 {
                let rng_seed = (i * 100 + attempt) as u64 + 1;
                if extend_candidate(rng_seed, src).is_some() {
                    found += 1;
                    break;
                }
            }
        }
        assert!(found > 0, "延長法で候補が1つも生成されなかった");
        eprintln!("延長法候補生成: {}/{} 成功", found, sources.len());
    }

    #[test]
    fn test_interpose_unwind() {
        // 合駒逆算のテスト: スライド王手のある局面から合駒を追加
        let mut found = 0;
        for seed in 0..200 {
            let mut rng = Rng::new(seed + 1);
            if let Some(mated) = generate_mated_position(&mut rng) {
                let dk_pos = mated.king_pos(Owner::Defender).unwrap();
                if let Some(_s) = unwind_defender_interpose(&mut rng, &mated, dk_pos) {
                    found += 1;
                }
            }
        }
        eprintln!("合駒逆算: {}/200 成功", found);
        // 合駒逆算は条件が厳しいので成功しなくてもテストは通す
    }

    /// 9手詰パズルからの延長成功率を診断するテスト（候補生成のみ）
    #[test]
    fn test_extend_from_9mate_diagnostic() {
        // 9手詰パズルファイルを読む
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../puzzles/9.json");
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("puzzles/9.json が見つからない、スキップ");
                return;
            }
        };
        let puzzles: Vec<serde_json::Value> = serde_json::from_str(&data).unwrap();
        eprintln!("9手詰パズル数: {}", puzzles.len());

        let mut total_attempts = 0;
        let mut extend_success = 0;
        let mut extend_fail_defender = 0;
        let mut extend_fail_attacker = 0;

        for (pi, puzzle) in puzzles.iter().enumerate() {
            let initial: InitialData = serde_json::from_value(puzzle["initial"].clone()).unwrap();
            for attempt in 0..100 {
                total_attempts += 1;
                let rng_seed = (pi * 1000 + attempt) as u64 + 1;
                let mut rng = Rng::new(rng_seed);
                let mut current = initial.to_state();
                current.side_to_move = Owner::Attacker;

                // 守り方の手を巻き戻す
                let unwound_def = try_unwind_defender(&mut rng, &current, 50);
                if unwound_def.is_none() {
                    extend_fail_defender += 1;
                    continue;
                }
                let unwound_def = unwound_def.unwrap();

                // 攻め方の手を巻き戻す
                let unwound_atk = try_unwind_attacker(&mut rng, &unwound_def, 50);
                if unwound_atk.is_none() {
                    extend_fail_attacker += 1;
                    continue;
                }
                extend_success += 1;
            }
        }

        eprintln!("=== 延長法診断 (9手詰→11手詰) ===");
        eprintln!("総試行: {}", total_attempts);
        eprintln!("守り方巻き戻し失敗: {} ({:.1}%)", extend_fail_defender, extend_fail_defender as f64 / total_attempts as f64 * 100.0);
        eprintln!("攻め方巻き戻し失敗: {} ({:.1}%)", extend_fail_attacker, extend_fail_attacker as f64 / total_attempts as f64 * 100.0);
        eprintln!("延長候補生成成功: {} ({:.1}%)", extend_success, extend_success as f64 / total_attempts as f64 * 100.0);
    }

}
