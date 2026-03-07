# DEVELOPMENT

このドキュメントは、たぬき詰将棋の開発者向けガイドです。

## 目的

- GitHub Pages で動く静的 Web アプリとして提供する
- スマホ操作を最優先にした UI/UX を実装する
- 詰将棋問題は事前生成し、配信時は JSON を読むだけにする

## 想定アーキテクチャ

- `frontend`:
  - タイトル画面
  - 手数カテゴリ画面（3/5/7/9）
  - 問題一覧画面（1〜100）
  - 対局画面（盤面、持ち駒、手順表示）
- `puzzle-data`:
  - 各手数カテゴリごとの問題 JSON
- `generator`:
  - 詰将棋問題の候補生成
- `validator`:
  - ルール適合性・唯一解・手数厳密性を検証

### 補足: 生成アルゴリズム

- ランダム生成のみでは品質が安定しないため、終局形からの逆算発想を取り入れる
- 逆算または近傍探索で得た候補を `validator` で厳密検証し、合格局面のみ採用する
- 参考: https://memo.sugyan.com/entry/2017/11/19/220631

## 重要方針

### 1. 問題生成と配信を分離する

アプリ実行時に問題生成は行わない。配信前に以下を実行する。

1. 生成エンジンで候補を作る
2. 検証エンジンで通す
3. 合格問題のみ JSON に出力
4. フロントエンドは JSON を読むだけ

### 2. 検証エンジンで保証すること

- 指定手数（3/5/7/9）ちょうどで詰む
- 短手数詰みではない
- 将棋の禁じ手が含まれない（例: 二歩、打ち歩詰め、行き所のない駒打ち）
- 解が一意である（攻め方の正解手順が一つ）

### 3. 進捗管理

- キー例: `tanuki-tsume:v1:clear:{mateLength}:{puzzleId}`
- 値: `true`
- 集計用にカテゴリ別クリア率を算出できる構造にする

## 予定ディレクトリ構成

```txt
.
├── README.md
├── DEVELOPMENT.md
├── docs/                 # GitHub Pages 公開物
├── src/
│   ├── app/              # フロントエンド
│   ├── shogi-core/       # 盤面・合法手・詰み判定の基礎ロジック
│   ├── generator/        # 問題生成
│   └── validator/        # 問題検証
├── puzzles/
│   ├── 3.json
│   ├── 5.json
│   ├── 7.json
│   └── 9.json
└── scripts/
    ├── generate.ts
    └── validate.ts
```

## 初期マイルストーン

1. 盤面表現と合法手生成（最低限ルール）
2. 詰み探索（指定手数）
3. 唯一解判定
4. CLI で問題生成・検証して JSON 出力
5. Web UI 実装（画面遷移、対局、クリア記録）
6. GitHub Pages へデプロイ

## テスト方針

- `shogi-core`: 単体テスト中心（駒移動、打ち、成り、禁じ手）
- `validator`: 既知問題セットで回帰テスト
- `frontend`: 主要動線の E2E（カテゴリ選択 -> 問題選択 -> クリア記録）

## リリース方針

- バージョン付き問題データを配信
- 問題差し替え時は互換性を崩さないように `localStorage` キーをバージョニングする

## 生成運用メモ

- 問題生成: `npm run generate`
- 問題検証: `npm run validate`
- 7手詰探索: `npm run mine:7 > /tmp/found7.json`
- 9手詰探索: `npm run mine:9 > /tmp/found9.json`
- 探索結果取り込み: `npm run add:curated -- --file=/tmp/found7.json`

`mine:*` は長時間実行を想定。見つかった局面だけ `data/curated-puzzles.json` に追記し、再生成する。
