# プロジェクト規約

- コメントとドキュメントは日本語で記述すること

# アーキテクチャ概要

## ディレクトリ構成

- `src/` — フロントエンドソース（Vite でビルド）
  - `src/shogi-core.js` — WASM ラッパー（Rust エンジンの薄いバインディング層）
  - `src/app.js` — UIコントローラ
  - `src/styles.css` — スタイルシート
  - `src/wasm-pkg/` — wasm-pack 生成物（.gitignore済み）
- `public/` — Vite の静的アセット（ビルド時にそのまま dist/ にコピー）
  - `public/puzzles/` — 生成済みパズルJSON
  - `public/assets/` — 画像等
- `shogi-core/` — Rust 将棋エンジン共有ライブラリ（Cargo workspace メンバー）
  - `shogi-core/src/shogi.rs` — 将棋ルール・合法手生成・詰み探索
  - `shogi-core/src/solver.rs` — 詰将棋ソルバー（forcedMateWithin / findBestDefense）
  - `shogi-core/src/dfpn.rs` — df-pn ソルバー
- `wasm/` — WASM バインディングクレート（wasm-bindgen、JSON境界）
- `generator/` — Rust製パズル生成・検証エンジン（rayon並列化）
  - `generator/src/generate.rs` — パズル生成ロジック（候補生成・変異・検証・並べ替え）
  - `generator/src/main.rs` — CLI エントリポイント（generate / validate）
- `scripts/` — Node.jsユーティリティ
- `tests/` — テスト
  - `tests/e2e/` — Playwright E2Eテスト
  - `tests/unit/` — Vitest ユニットテスト
- `data/` — curated-puzzles.json（生成のシード局面）
- `docs/` — ドキュメント
  - `docs/generator.md` — 生成エンジンの実装解説
  - `docs/tsume-rules.md` — 詰将棋ルール整理（合駒・無駄合いなど）
  - `docs/generation-algorithms.md` — 生成アルゴリズム調査と改善方針

## ビルド・テスト

- `npm run dev` — WASM ビルド → Vite 開発サーバー起動
- `npm run build` — WASM ビルド → パズル検証 → Vite ビルド（dist/ に出力）
- `npm run build:wasm` — wasm-pack ビルド（src/wasm-pkg/ に出力）
- `npm run preview` — ビルド済み dist/ をプレビュー
- `npm run test:e2e` — Playwright E2E テスト（事前に `vite build` が必要）
- `npx vitest run` — JS ユニットテスト（WASM 経由で Rust エンジンをテスト）
- `cargo test` — Rust ユニットテスト（ワークスペースルートから実行）
- `cargo clippy -- -D warnings` — Rust lint
- CI: check / e2e / rust の3ジョブ並列 + pages デプロイ

## パズル生成

- `cd generator && cargo run --release -- generate` — パズル生成（インクリメンタル）
- `cd generator && cargo run --release -- validate` — 既存パズルの検証
- 既存の有効パズルを保持し、新規発見分を追加する蓄積方式
- パズルJSON は `puzzles/` と `public/puzzles/` の両方に同一内容を出力
- 各パズルにハッシュID（初期局面から算出した8文字hex）を付与
- URLは `?mate=N&pid=HASH` 形式（`?id=N` も後方互換で動作）

## 詰将棋ルール

- 攻め方は王手の連続、守り方は最長抵抗
- 守り方の持ち駒 = 駒箱（盤上と攻め方持ち駒以外の全駒を自動計算）
- 唯一解、最短手数、駒余りなし
- 無駄合い判定: スライド王手に対する合駒→同Xで詰むなら無駄合いとしてスキップ

## 注意事項

- 将棋ロジックは Rust（shogi-core）に一元化し、WASM 経由でブラウザから利用
- WASM 境界は JSON 文字列でやり取り（JS 側で Map に変換）
- ビルド情報は `vite.config.js` の `define` で `__BUILD_INFO__` としてコンパイル時注入
- E2Eテストはハッシュベースの動的パズル参照を使用（再生成でも壊れない）
- Cargo workspace 構成: shogi-core（共有lib）、wasm（cdylib）、generator（CLI）
