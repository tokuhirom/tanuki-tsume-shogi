mod shogi;
mod generate;

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;

fn parse_generate_args(args: &[String]) -> (u32, u32, u32, u32, u64) {
    let mut max: u32 = 100;
    let mut attempts1: u32 = 100_000;
    let mut attempts3: u32 = 200_000;
    let mut attempts5: u32 = 100_000;
    let mut seed: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        % 2_147_483_647;

    for a in args {
        if let Some(v) = a.strip_prefix("--max=") {
            max = v.parse().unwrap_or(max);
        }
        if let Some(v) = a.strip_prefix("--seed=") {
            seed = v.parse().unwrap_or(seed);
        }
        if let Some(v) = a.strip_prefix("--attempts1=") {
            attempts1 = v.parse().unwrap_or(attempts1);
        }
        if let Some(v) = a.strip_prefix("--attempts3=") {
            attempts3 = v.parse().unwrap_or(attempts3);
        }
        if let Some(v) = a.strip_prefix("--attempts5=") {
            attempts5 = v.parse().unwrap_or(attempts5);
        }
    }
    (max, attempts1, attempts3, attempts5, seed)
}

fn run_generate(args: &[String]) {
    let (max, attempts1, attempts3, attempts5, seed) = parse_generate_args(args);

    let curated = generate::load_curated("data/curated-puzzles.json");

    for mate_len in [1u32, 3, 5] {
        let attempts = match mate_len {
            1 => attempts1,
            3 => attempts3,
            _ => attempts5,
        };
        let seeds = curated.get(&mate_len).cloned().unwrap_or_default();
        let puzzles = generate::generate_puzzles(seed, mate_len, attempts, &seeds, max);

        let json = serde_json::to_string_pretty(&puzzles).unwrap();

        for dir in &["puzzles", "public/puzzles"] {
            let dir_path = Path::new(dir);
            fs::create_dir_all(dir_path).ok();
            let file = dir_path.join(format!("{}.json", mate_len));
            fs::write(&file, &json).unwrap();
        }

        eprintln!("{}手詰: {}問 (attempts={})", mate_len, puzzles.len(), attempts);
    }

    eprintln!("seed={}", seed);
}

/// パズルデータを読み込んで詰将棋として正しいか検証する
fn cmd_validate() {
    let mut failed = 0u32;

    for mate_len in [1u32, 3, 5] {
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
        validate_file(target, mate_len, &mut failed);
    }

    if failed > 0 {
        std::process::exit(1);
    }
}

fn validate_file(file: &str, mate_len: u32, failed: &mut u32) {
    let data = fs::read_to_string(file).unwrap();
    let puzzles: Vec<generate::Puzzle> = serde_json::from_str(&data).unwrap();

    let mut checked = HashSet::new();
    for p in &puzzles {
        let sig = serde_json::to_string(&p.initial).unwrap();
        if checked.contains(&sig) {
            continue;
        }
        checked.insert(sig);

        let mut state = p.initial.to_state();
        let result = shogi::validate_tsume_puzzle(&mut state, mate_len);
        if result.is_none() {
            eprintln!("[NG] {}手詰 #{}: 詰将棋として不正", mate_len, p.id);
            *failed += 1;
            return;
        }
    }
    eprintln!("[OK] {}: {}問 (unique {})", file, puzzles.len(), checked.len());
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let subcommand = args.get(1).map(|s| s.as_str()).unwrap_or("generate");

    match subcommand {
        "validate" => cmd_validate(),
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
