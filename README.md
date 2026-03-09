# たぬき詰将棋

<p align="center">
  <img src="docs/assets/tanuki.svg" alt="たぬき詰将棋のタヌキ" width="280">
</p>

スマホでサクサク遊べる、GitHub Pages ホスティング前提の Web 詰将棋アプリです。

👉 **[遊ぶ](https://tokuhirom.github.io/tanuki-tsume-shogi/)**

> [!WARNING]
> このアプリは現在開発中です。仕様・問題データ・画面UIは予告なく変更される場合があります。

## 特徴

- ブラウザだけでプレイ可能（インストール不要）
- 1手詰〜11手詰を収録（問題数は手数により異なる）
- 将棋ロジックは Rust で実装し、WASM 経由でブラウザから利用
- Rust製パズル生成エンジンで自動生成（逆算法・延長法・変異法を併用）
- 不正解時に理由（取られる/逃げられる/合駒）をヒントマス付きで表示
- 駒余りなし・唯一解・駒箱ルール対応
- クリア状況を `localStorage` に保存
- タヌキテーマのタイトル画面・UI

## 遊び方

1. タイトル画面で手数（1手詰〜11手詰）を選択
2. 問題一覧から問題を選択
3. 盤面で詰将棋を解く
4. クリアすると自動で進捗が保存される

## 対応環境

- iOS / Android のモバイルブラウザ（最新の Safari / Chrome 推奨）
- PC ブラウザでも動作

## 進捗データ

- クリア状況はブラウザの `localStorage` に保存されます
- ブラウザのデータ削除や端末変更で進捗は失われます

## 開発

```bash
npm run dev          # WASM ビルド → Vite 開発サーバー起動
npm run build        # WASM ビルド → パズル検証 → Vite ビルド
cargo test           # Rust ユニットテスト
npx vitest run       # JS ユニットテスト
npx playwright test  # E2E テスト（事前に vite build が必要）
```

### パズル生成

```bash
cd generator && cargo run --release -- generate --seed=12345
cd generator && cargo run --release -- validate
```

## GitHub Pages

- `main` ブランチへの push で自動デプロイ（`.github/workflows/pages.yml`）
- リポジトリ設定の `Settings > Pages > Build and deployment` は `GitHub Actions` を選択してください

## ライセンス

MIT (LICENSE 参照)
