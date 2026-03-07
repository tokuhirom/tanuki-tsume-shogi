# プロジェクト規約

- コメントとドキュメントは日本語で記述すること

# currentDate
Today's date is 2026-03-08.

# アーキテクチャ概要

## ディレクトリ構成

- `docs/` — GitHub Pages で公開されるフロントエンド (静的HTML+JS+CSS)
  - `docs/shogi-core.js` — 将棋ルールエンジン（唯一の実装。ブラウザ・Node 両方から使用）
  - `docs/app.js` — UIコントローラ
  - `docs/puzzles/` — 生成済みパズルJSON
- `generator/` — Rust製パズル生成エンジン（rayon並列化）
- `scripts/` — Node.jsユーティリティ
- `tests/e2e/` — Playwright E2Eテスト
- `data/` — curated-puzzles.json（生成のシード局面）

## ビルド・テスト

- `npm run build` — パズル生成→検証→ビルド情報書き出しの一括実行
- `npm run test:e2e` — Playwright E2E テスト
- `cd generator && cargo test` — Rust ユニットテスト
- CI: check / e2e / rust の3ジョブ並列

## 注意事項

- パズルJSON は `puzzles/` と `docs/puzzles/` の両方に同一内容を出力する
