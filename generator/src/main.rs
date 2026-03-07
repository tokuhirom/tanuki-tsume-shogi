mod shogi;
mod generate;

use std::env;
use std::fs;
use std::path::Path;

fn parse_args() -> (u32, u32, u32, u32, u64) {
    let mut max: u32 = 100;
    let mut attempts1: u32 = 100_000;
    let mut attempts3: u32 = 200_000;
    let mut attempts5: u32 = 100_000;
    let mut seed: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        % 2_147_483_647;

    for a in env::args().skip(1) {
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

fn main() {
    let (max, attempts1, attempts3, attempts5, seed) = parse_args();

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

        for dir in &["puzzles", "docs/puzzles"] {
            let dir_path = Path::new(dir);
            fs::create_dir_all(dir_path).ok();
            let file = dir_path.join(format!("{}.json", mate_len));
            fs::write(&file, &json).unwrap();
        }

        eprintln!("{}手詰: {}問 (attempts={})", mate_len, puzzles.len(), attempts);
    }

    eprintln!("seed={}", seed);
}
