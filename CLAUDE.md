# プロジェクト規約

- コメントとドキュメントは日本語で記述すること

# アーキテクチャ概要

## ディレクトリ構成

- `src/` — フロントエンドソース（Vite でビルド）
  - `src/shogi-core.js` — 将棋ルールエンジン（唯一の実装。ブラウザ・Node 両方から使用）
  - `src/app.js` — UIコントローラ
  - `src/styles.css` — スタイルシート
- `public/` — Vite の静的アセット（ビルド時にそのまま dist/ にコピー）
  - `public/puzzles/` — 生成済みパズルJSON
  - `public/assets/` — 画像等
- `generator/` — Rust製パズル生成・検証エンジン（rayon並列化）
  - `generator/src/generate.rs` — パズル生成ロジック（候補生成・変異・検証・並べ替え）
  - `generator/src/shogi.rs` — 将棋ルール・合法手生成・詰み探索
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

- `npm run dev` — Vite 開発サーバー起動
- `npm run build` — パズル生成→検証→Viteビルド（dist/ に出力）
- `npm run preview` — ビルド済み dist/ をプレビュー
- `npm run test:e2e` — Playwright E2E テスト（事前に `vite build` が必要）
- `npx vitest run` — JS ユニットテスト
- `cd generator && cargo test` — Rust ユニットテスト
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
- 無駄合い判定は未実装（→ `docs/tsume-rules.md` 参照）

## 注意事項

- ビルド情報は `vite.config.js` の `define` で `__BUILD_INFO__` としてコンパイル時注入
- E2Eテストはハッシュベースの動的パズル参照を使用（再生成でも壊れない）
- Rust/JS両エンジンで守り方の応手選択ロジックを `>=`（最後の同長手選択）で統一
