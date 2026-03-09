mod generate;
mod backward;

use shogi_core::shogi;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;

/// 生成方式
#[derive(Clone, Copy, PartialEq)]
pub enum GenerateMethod {
    Random,   // ランダム法（既存方式）
    Backward, // 逆算法（バックワード方式）
    Both,     // 両方を併用
}

struct GenerateArgs {
    max: u32,
    attempts: [u32; 6],  // 1手, 3手, 5手, 7手, 9手, 11手
    seed: u64,
    only: Option<u32>,   // 特定の手数だけ生成する場合
    method: GenerateMethod,
}

const ALL_MATE_LENGTHS: [u32; 6] = [1, 3, 5, 7, 9, 11];

fn parse_generate_args(args: &[String]) -> GenerateArgs {
    let mut ga = GenerateArgs {
        max: u32::MAX,  // 上限なし（見つかっただけ収録）
        attempts: [100_000, 200_000, 100_000, 200_000, 200_000, 200_000],
        seed: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            % 2_147_483_647,
        only: None,
        method: GenerateMethod::Both,
    };

    for a in args {
        if let Some(v) = a.strip_prefix("--max=") {
            ga.max = v.parse().unwrap_or(ga.max);
        } else if let Some(v) = a.strip_prefix("--seed=") {
            ga.seed = v.parse().unwrap_or(ga.seed);
        } else if let Some(v) = a.strip_prefix("--only=") {
            ga.only = v.parse().ok();
        } else if let Some(v) = a.strip_prefix("--attempts1=") {
            ga.attempts[0] = v.parse().unwrap_or(ga.attempts[0]);
        } else if let Some(v) = a.strip_prefix("--attempts3=") {
            ga.attempts[1] = v.parse().unwrap_or(ga.attempts[1]);
        } else if let Some(v) = a.strip_prefix("--attempts5=") {
            ga.attempts[2] = v.parse().unwrap_or(ga.attempts[2]);
        } else if let Some(v) = a.strip_prefix("--attempts7=") {
            ga.attempts[3] = v.parse().unwrap_or(ga.attempts[3]);
        } else if let Some(v) = a.strip_prefix("--attempts9=") {
            ga.attempts[4] = v.parse().unwrap_or(ga.attempts[4]);
        } else if let Some(v) = a.strip_prefix("--attempts11=") {
            ga.attempts[5] = v.parse().unwrap_or(ga.attempts[5]);
        } else if let Some(v) = a.strip_prefix("--method=") {
            ga.method = match v {
                "random" => GenerateMethod::Random,
                "backward" => GenerateMethod::Backward,
                "both" => GenerateMethod::Both,
                _ => {
                    eprintln!("不明な生成方式: {} (random/backward/both)", v);
                    ga.method
                }
            };
        }
    }
    ga
}

fn run_generate(args: &[String]) {
    let ga = parse_generate_args(args);

    let curated = generate::load_curated("data/curated-puzzles.json");

    // 前の手数の結果を保持（延長法で使用）
    let mut prev_puzzles: Vec<generate::Puzzle> = vec![];

    for (i, &mate_len) in ALL_MATE_LENGTHS.iter().enumerate() {
        if let Some(only) = ga.only {
            if mate_len != only {
                // --only 指定時でも、延長法のため前の手数の結果を読み込む
                if mate_len == only.saturating_sub(2) {
                    let file = format!("puzzles/{}.json", mate_len);
                    if Path::new(&file).exists() {
                        let data = fs::read_to_string(&file).unwrap_or_default();
                        prev_puzzles = serde_json::from_str(&data).unwrap_or_default();
                    }
                }
                continue;
            }
        }
        let attempts = ga.attempts[i];
        let seeds = curated.get(&mate_len).cloned().unwrap_or_default();

        // 既存パズルを読み込み（インクリメンタル生成用）
        let existing: Vec<generate::Puzzle> = {
            let file = format!("puzzles/{}.json", mate_len);
            if Path::new(&file).exists() {
                let data = fs::read_to_string(&file).unwrap_or_default();
                serde_json::from_str(&data).unwrap_or_default()
            } else {
                vec![]
            }
        };

        // 延長法のソース: 1手詰には使えない（前の手数がない）
        let shorter = if mate_len > 1 { &prev_puzzles } else { &vec![] };

        let puzzles = generate::generate_puzzles(ga.seed, mate_len, attempts, &seeds, ga.max, &existing, ga.method, shorter);

        let json = serde_json::to_string_pretty(&puzzles).unwrap();

        for dir in &["puzzles", "public/puzzles"] {
            let dir_path = Path::new(dir);
            fs::create_dir_all(dir_path).ok();
            let file = dir_path.join(format!("{}.json", mate_len));
            fs::write(&file, &json).unwrap();
        }

        eprintln!("{}手詰: {}問 (attempts={})", mate_len, puzzles.len(), attempts);

        // 次の手数の延長法用に保持
        prev_puzzles = puzzles;
    }

    eprintln!("seed={}", ga.seed);
}

/// パズルデータを読み込んで詰将棋として正しいか検証する
fn cmd_validate(fix: bool) {
    let mut failed = 0u32;

    for mate_len in ALL_MATE_LENGTHS {
        let file = format!("puzzles/{}.json", mate_len);
        let alt = format!("docs/puzzles/{}.json", mate_len);
        let target = if Path::new(&file).exists() {
            &file
        } else if Path::new(&alt).exists() {
            &alt
        } else {
            eprintln!("[NG] {} が存在しません", file);
            failed += 1;
            continue;
        };
        validate_file(target, mate_len, &mut failed, fix);
    }

    if failed > 0 && !fix {
        std::process::exit(1);
    }
}

fn validate_file(file: &str, mate_len: u32, failed: &mut u32, fix: bool) {
    let data = fs::read_to_string(file).unwrap();
    let puzzles: Vec<generate::Puzzle> = serde_json::from_str(&data).unwrap();
    let original_count = puzzles.len();

    let mut checked = HashSet::new();
    let mut valid_puzzles: Vec<generate::Puzzle> = Vec::new();

    for p in &puzzles {
        let sig = serde_json::to_string(&p.initial).unwrap();
        if checked.contains(&sig) {
            continue;
        }
        checked.insert(sig);

        let mut state = p.initial.to_state();
        let result = shogi::validate_tsume_puzzle(&mut state, mate_len);
        match result {
            None => {
                eprintln!("[NG] {}手詰 #{}: 詰将棋として不正", mate_len, p.id);
                *failed += 1;
            }
            Some(solution) => {
                // 手順長チェック
                if solution.len() != mate_len as usize {
                    eprintln!("[NG] {}手詰 #{}: 手順長が不正 ({}手)", mate_len, p.id, solution.len());
                    *failed += 1;
                    continue;
                }
                // 駒余りチェック: 全手順を適用した最終局面で攻め方の持ち駒が残っていないか
                let mut final_state = p.initial.to_state();
                for m in &solution {
                    final_state = shogi::apply_move(&final_state, m);
                }
                if final_state.hands.attacker.iter().sum::<u8>() > 0 {
                    eprintln!("[NG] {}手詰 #{}: 駒余りあり (持ち駒: {:?})", mate_len, p.id, final_state.hands.attacker);
                    *failed += 1;
                } else {
                    valid_puzzles.push(p.clone());
                }
            }
        }
    }

    if fix && valid_puzzles.len() < original_count {
        let removed = original_count - valid_puzzles.len();
        // IDを振り直す
        for (i, p) in valid_puzzles.iter_mut().enumerate() {
            p.id = (i + 1) as u32;
        }
        let json = serde_json::to_string_pretty(&valid_puzzles).unwrap();
        fs::write(file, &json).unwrap();
        // public/puzzles/ にもコピー
        let public_file = file.replace("puzzles/", "public/puzzles/");
        if Path::new(&public_file).parent().is_some_and(|d| d.exists()) {
            fs::write(&public_file, &json).unwrap();
        }
        eprintln!("[FIX] {}: {}問削除 → {}問に修正", file, removed, valid_puzzles.len());
    } else {
        eprintln!("[OK] {}: {}問 (unique {})", file, puzzles.len(), checked.len());
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let subcommand = args.get(1).map(|s| s.as_str()).unwrap_or("generate");

    match subcommand {
        "validate" => cmd_validate(false),
        "validate-fix" => cmd_validate(true),
        "generate" => run_generate(&args[2..]),
        // 後方互換: --seed= 等の引数が直接来た場合はgenerate扱い
        _ if subcommand.starts_with("--") => run_generate(&args[1..]),
        other => {
            eprintln!("不明なサブコマンド: {}", other);
            eprintln!("使い方: tsume-gen [generate|validate] [オプション]");
            std::process::exit(1);
        }
    }
}
