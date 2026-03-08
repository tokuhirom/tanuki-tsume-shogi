# プロジェクト規約

- コメントとドキュメントは日本語で記述すること

# currentDate
Today's date is 2026-03-08.

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
- `scripts/` — Node.jsユーティリティ
- `tests/e2e/` — Playwright E2Eテスト
- `data/` — curated-puzzles.json（生成のシード局面）

## ビルド・テスト

- `npm run dev` — Vite 開発サーバー起動
- `npm run build` — パズル生成→検証→Viteビルド（dist/ に出力）
- `npm run preview` — ビルド済み dist/ をプレビュー
- `npm run test:e2e` — Playwright E2E テスト（事前に `vite build` が必要）
- `cd generator && cargo test` — Rust ユニットテスト
- CI: check / e2e / rust の3ジョブ並列 + pages デプロイ

## 注意事項

- パズルJSON は `puzzles/` と `public/puzzles/` の両方に同一内容を出力する
- ビルド情報は `vite.config.js` の `define` で `__BUILD_INFO__` としてコンパイル時注入
