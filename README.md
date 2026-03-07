# たぬき詰将棋

<p align="center">
  <img src="docs/assets/tanuki.svg" alt="たぬき詰将棋のタヌキ" width="280">
</p>

スマホでサクサク遊べる、GitHub Pages ホスティング前提の Web 詰将棋アプリです。

> [!WARNING]
> このアプリは現在開発中です。仕様・問題データ・画面UIは予告なく変更される場合があります。

## 特徴

- ブラウザだけでプレイ可能（インストール不要）
- 3手詰・5手詰・7手詰・9手詰に対応
- 各カテゴリに 1〜100 の問題を収録
- ステージ選択式（タイトル画面 -> 手数選択 -> 問題番号選択）
- クリア状況を `localStorage` に保存
- タヌキテーマのタイトル画面・UI

## 遊び方

1. タイトル画面で「3手詰 / 5手詰 / 7手詰 / 9手詰」を選択
2. 問題一覧から番号（1〜100）を選択
3. 盤面で詰将棋を解く
4. クリアすると自動で進捗が保存される

## 対応環境

- iOS / Android のモバイルブラウザ（最新の Safari / Chrome 推奨）
- PC ブラウザでも動作

## 進捗データ

- クリア状況はブラウザの `localStorage` に保存されます
- ブラウザのデータ削除や端末変更で進捗は失われます

## 開発

- 開発者向け情報は [DEVELOPMENT.md](./DEVELOPMENT.md) を参照

## GitHub Pages

- `main` ブランチへの push で `docs/` を自動デプロイします（`.github/workflows/pages.yml`）
- リポジトリ設定の `Settings > Pages > Build and deployment` は `GitHub Actions` を選択してください

## ライセンス

MIT (LICENSE 参照)
