use wasm_bindgen::prelude::*;
use shogi_core::shogi::*;
use shogi_core::solver;

/// JS 用 State: board は {"x,y": {owner, type}} 形式のオブジェクト
/// WASM の境界では JSON 文字列でやり取りし、JS 側で Map に変換する
/// 初期データ JSON から State を作成し、JSON 文字列で返す
#[wasm_bindgen(js_name = "createState")]
pub fn create_state(initial_json: &str) -> Result<String, JsValue> {
    let initial: InitialData = serde_json::from_str(initial_json)
        .map_err(|e| JsValue::from_str(&format!("JSON parse error: {}", e)))?;
    let state = initial.to_state();
    Ok(state_to_json(&state))
}

/// 手を適用して新しい状態 JSON を返す
#[wasm_bindgen(js_name = "applyMoveW")]
pub fn apply_move_w(state_json: &str, move_json: &str) -> Result<String, JsValue> {
    let state = json_to_state(state_json)?;
    let m: Move = serde_json::from_str(move_json)
        .map_err(|e| JsValue::from_str(&format!("Move parse error: {}", e)))?;
    let next = apply_move(&state, &m);
    Ok(state_to_json(&next))
}

/// 合法手を JSON 配列で返す
#[wasm_bindgen(js_name = "legalMovesW")]
pub fn legal_moves_w(state_json: &str) -> Result<String, JsValue> {
    let mut state = json_to_state(state_json)?;
    let mut moves = legal_board_moves(&mut state);
    moves.extend(legal_drop_moves(&mut state));
    serde_json::to_string(&moves)
        .map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e)))
}

/// 王手判定
/// JS 旧版互換: 守り方の玉がない場合は true を返す
#[wasm_bindgen(js_name = "isInCheckW")]
pub fn is_in_check_w(state_json: &str, owner: &str) -> Result<bool, JsValue> {
    let state = json_to_state(state_json)?;
    let o = parse_owner(owner)?;
    // 守り方の玉がない場合、JS の旧実装では "defender" に対して true を返す
    if o == Owner::Defender && state.king_pos(Owner::Defender).is_none() {
        return Ok(true);
    }
    Ok(is_in_check(&state, o))
}

/// 守り方の最善応手を返す（JSON）
#[wasm_bindgen(js_name = "findBestDefenseW")]
pub fn find_best_defense_w(state_json: &str, remaining_plies: u32) -> Result<String, JsValue> {
    let mut state = json_to_state(state_json)?;
    match solver::find_best_defense(&mut state, remaining_plies) {
        Some(m) => serde_json::to_string(&m)
            .map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e))),
        None => Ok("null".to_string()),
    }
}

/// 詰将棋パズル検証（解があれば手順 JSON 配列、なければ "null"）
#[wasm_bindgen(js_name = "validateTsumePuzzleW")]
pub fn validate_tsume_puzzle_w(state_json: &str, mate_length: u32) -> Result<String, JsValue> {
    let mut state = json_to_state(state_json)?;
    match solver::validate_tsume_puzzle_js(&mut state, mate_length) {
        Some(moves) => serde_json::to_string(&moves)
            .map_err(|e| JsValue::from_str(&format!("Serialize error: {}", e))),
        None => Ok("null".to_string()),
    }
}

/// 玉の位置を返す（JSON: {x, y} or "null"）
#[wasm_bindgen(js_name = "kingPosW")]
pub fn king_pos_w(state_json: &str, owner: &str) -> Result<String, JsValue> {
    let state = json_to_state(state_json)?;
    let o = parse_owner(owner)?;
    match state.king_pos(o) {
        Some(p) => Ok(format!("{{\"x\":{},\"y\":{}}}", p.x, p.y)),
        None => Ok("null".to_string()),
    }
}

// --- 内部ヘルパー ---

/// State を JSON 文字列に変換
/// フォーマット: {"board":{"x,y":{"owner":"attacker","type":"G"},...}, "hands":{...}, "sideToMove":"attacker"}
fn state_to_json(state: &State) -> String {
    let mut board_entries = Vec::new();
    for y in 1..=9i8 {
        for x in 1..=9i8 {
            if let Some(bp) = state.get(Pos::new(x, y)) {
                // 詰将棋: 攻め方の玉は盤面に含めない
                if bp.owner == Owner::Attacker && bp.piece_type == PieceType::K {
                    continue;
                }
                let owner_str = match bp.owner {
                    Owner::Attacker => "attacker",
                    Owner::Defender => "defender",
                };
                let type_str = piece_type_to_str(bp.piece_type);
                board_entries.push(format!("\"{},{}\":{{\"owner\":\"{}\",\"type\":\"{}\"}}", x, y, owner_str, type_str));
            }
        }
    }
    let board_json = format!("{{{}}}", board_entries.join(","));

    let hands_json = format!(
        "{{\"attacker\":{{\"R\":{},\"B\":{},\"G\":{},\"S\":{},\"N\":{},\"L\":{},\"P\":{}}},\"defender\":{{\"R\":{},\"B\":{},\"G\":{},\"S\":{},\"N\":{},\"L\":{},\"P\":{}}}}}",
        state.hands.attacker[0], state.hands.attacker[1], state.hands.attacker[2],
        state.hands.attacker[3], state.hands.attacker[4], state.hands.attacker[5], state.hands.attacker[6],
        state.hands.defender[0], state.hands.defender[1], state.hands.defender[2],
        state.hands.defender[3], state.hands.defender[4], state.hands.defender[5], state.hands.defender[6],
    );

    let side = match state.side_to_move {
        Owner::Attacker => "attacker",
        Owner::Defender => "defender",
    };

    format!("{{\"board\":{},\"hands\":{},\"sideToMove\":\"{}\"}}", board_json, hands_json, side)
}

/// JSON 文字列から State を復元
fn json_to_state(json: &str) -> Result<State, JsValue> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| JsValue::from_str(&format!("JSON parse error: {}", e)))?;

    let mut state = State::new();

    // board
    if let Some(board) = v["board"].as_object() {
        for (key, val) in board {
            let parts: Vec<&str> = key.split(',').collect();
            if parts.len() != 2 { continue; }
            let x: i8 = parts[0].parse().unwrap_or(0);
            let y: i8 = parts[1].parse().unwrap_or(0);
            let owner = parse_owner(val["owner"].as_str().unwrap_or("attacker"))
                .unwrap_or(Owner::Attacker);
            let piece_type = str_to_piece_type(val["type"].as_str().unwrap_or("P"));
            state.set(Pos::new(x, y), Some(BoardPiece { owner, piece_type }));
        }
    }

    // hands
    if let Some(hands) = v["hands"].as_object() {
        if let Some(atk) = hands.get("attacker").and_then(|v| v.as_object()) {
            state.hands.attacker = parse_hand_array(atk);
        }
        if let Some(def) = hands.get("defender").and_then(|v| v.as_object()) {
            state.hands.defender = parse_hand_array(def);
        }
    }

    // sideToMove
    state.side_to_move = parse_owner(v["sideToMove"].as_str().unwrap_or("attacker"))
        .unwrap_or(Owner::Attacker);

    state.zobrist_hash = state.compute_zobrist();
    Ok(state)
}

fn parse_hand_array(obj: &serde_json::Map<String, serde_json::Value>) -> [u8; 7] {
    [
        obj.get("R").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
        obj.get("B").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
        obj.get("G").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
        obj.get("S").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
        obj.get("N").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
        obj.get("L").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
        obj.get("P").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
    ]
}

fn parse_owner(s: &str) -> Result<Owner, JsValue> {
    match s {
        "attacker" => Ok(Owner::Attacker),
        "defender" => Ok(Owner::Defender),
        _ => Err(JsValue::from_str(&format!("Invalid owner: {}", s))),
    }
}

fn piece_type_to_str(pt: PieceType) -> &'static str {
    match pt {
        PieceType::K => "K",
        PieceType::R => "R",
        PieceType::B => "B",
        PieceType::G => "G",
        PieceType::S => "S",
        PieceType::N => "N",
        PieceType::L => "L",
        PieceType::P => "P",
        PieceType::PR => "+R",
        PieceType::PB => "+B",
        PieceType::PS => "+S",
        PieceType::PN => "+N",
        PieceType::PL => "+L",
        PieceType::PP => "+P",
    }
}

fn str_to_piece_type(s: &str) -> PieceType {
    match s {
        "K" => PieceType::K,
        "R" => PieceType::R,
        "B" => PieceType::B,
        "G" => PieceType::G,
        "S" => PieceType::S,
        "N" => PieceType::N,
        "L" => PieceType::L,
        "P" => PieceType::P,
        "+R" => PieceType::PR,
        "+B" => PieceType::PB,
        "+S" => PieceType::PS,
        "+N" => PieceType::PN,
        "+L" => PieceType::PL,
        "+P" => PieceType::PP,
        _ => PieceType::P,
    }
}
