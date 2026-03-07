# プロジェクト規約

- コメントとドキュメントは日本語で記述すること

# currentDate
Today's date is 2026-03-08.

# アーキテクチャ概要

## ディレクトリ構成

- `docs/` — GitHub Pages で公開されるフロントエンド (静的HTML+JS+CSS)
  - `docs/shogi-core.js` — 将棋ルールエンジン（ブラウザ用、N/L対応済み）
  - `docs/app.js` — UIコントローラ
  - `docs/puzzles/` — 生成済みパズルJSON
- `src/shogi-core.js` — 将棋ルールエンジン（Node用、validate.js等が使用）
  - **注意**: `docs/shogi-core.js` と同期を保つこと
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

- `src/shogi-core.js` と `docs/shogi-core.js` は同じロジックだが別ファイル。変更時は両方を更新すること
- パズルJSON は `puzzles/` と `docs/puzzles/` の両方に同一内容を出力する
