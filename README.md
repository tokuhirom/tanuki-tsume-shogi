# たぬき詰将棋

<p align="center">
  <img src="public/assets/tanuki-title.png" alt="たぬき詰将棋のタヌキ" width="280">
</p>

スマホでサクサク遊べる、GitHub Pages ホスティング前提の Web 詰将棋アプリです。

👉 **[遊ぶ](https://tokuhirom.github.io/tanuki-tsume-shogi/)**

## 特徴

- ブラウザだけでプレイ可能（インストール不要）
- 1手詰〜11手詰を収録
- 将棋ロジックは Rust で実装し、WASM 経由でブラウザから利用
- Rust 製パズル生成エンジンで自動生成（逆算法・延長法・変異法を併用）
- 不正解時に理由（取られる/逃げられる/合駒）をヒントマス付きで表示
- 駒余りなし・唯一解・駒箱ルール対応
- クリア状況を `localStorage` に保存

## 遊び方

1. タイトル画面で手数（1手詰〜11手詰）を選択
2. 問題一覧から問題を選択
3. 盤面で詰将棋を解く
4. クリアすると自動で進捗が保存される

## ソースコード構成

```
├── src/                    # フロントエンド（Vite でビルド）
│   ├── app.js              # UI コントローラ
│   ├── shogi-core.js       # WASM ラッパー（Rust エンジンの薄いバインディング層）
│   ├── styles.css           # スタイルシート
│   └── wasm-pkg/           # wasm-pack 生成物（.gitignore 済み）
│
├── shogi-core/             # Rust 将棋エンジン共有ライブラリ
│   └── src/
│       ├── shogi.rs        # 将棋ルール・合法手生成・詰み探索
│       ├── solver.rs       # 詰将棋ソルバー（全幅探索）
│       ├── dfpn.rs         # df-pn ソルバー（7手詰以上で使用）
│       └── rng.rs          # 共有乱数生成器（xorshift）
│
├── wasm/                   # WASM バインディングクレート（wasm-bindgen）
│
├── generator/              # Rust 製パズル生成・検証エンジン
│   └── src/
│       ├── main.rs         # CLI エントリポイント（generate / validate）
│       ├── generate.rs     # パズル生成ロジック
│       └── backward.rs     # 逆算法・延長法による候補生成
│
├── scripts/                # Node.js ユーティリティ
│   └── filter-puzzles.js   # パズル多様性フィルタ（ビルド時に実行）
│
├── puzzles/                # 生成済みパズル JSON（生データ、全問保持）
├── public/puzzles/         # フィルタ適用後のパズル JSON（デプロイ用）
├── data/                   # curated-puzzles.json（生成のシード局面）
│
├── tests/
│   ├── e2e/                # Playwright E2E テスト
│   └── unit/               # Vitest ユニットテスト
│
└── docs/                   # 技術ドキュメント
    ├── tsume-rules.md      # 詰将棋ルール整理
    ├── generator.md        # 生成エンジンの実装解説
    ├── generation-algorithms.md  # 生成アルゴリズム調査
    └── dfpn-speedup.md     # df-pn 高速化調査
```

### 技術スタック

| レイヤー | 技術 |
|----------|------|
| 将棋エンジン | Rust（shogi-core クレート） |
| ブラウザ連携 | wasm-bindgen → WASM、JSON 境界 |
| フロントエンド | Vanilla JS + Vite |
| パズル生成 | Rust（generator クレート、rayon 並列化） |
| テスト | Vitest（ユニット）、Playwright（E2E）、cargo test（Rust） |
| CI/CD | GitHub Actions（check / e2e / rust の3ジョブ並列 + Pages デプロイ） |
| ホスティング | GitHub Pages |

## 開発

### 必要なツール

- Rust（stable）+ wasm-pack
- Node.js 20+
- npm

### コマンド

```bash
npm run dev          # WASM ビルド → Vite 開発サーバー起動
npm run build        # WASM ビルド → パズル検証 → 多様性フィルタ → Vite ビルド
cargo test           # Rust ユニットテスト
npx vitest run       # JS ユニットテスト
npx playwright test  # E2E テスト（事前に vite build が必要）
cargo clippy -- -D warnings  # Rust lint
```

### パズル生成

```bash
# パズル生成（インクリメンタル、既存パズルを保持しつつ新規追加）
cd generator && cargo run --release -- generate --seed=12345

# 特定の手数だけ生成
cd generator && cargo run --release -- generate --only=9 --seed=12345

# 既存パズルの検証
cd generator && cargo run --release -- validate
```

生成されたパズルは `puzzles/` に保存され、ビルド時に `scripts/filter-puzzles.js` で詰み筋の多様性フィルタを適用した結果が `public/puzzles/` に出力されます。

### ビルドパイプライン

```
cargo generate → puzzles/*.json（生データ、全問保持）
                      ↓
           filter-puzzles.js（詰み筋の類似排除）
                      ↓
              public/puzzles/*.json（デプロイ用）
                      ↓
                  vite build → dist/
```

## ドキュメント

プロジェクトの技術的な詳細は `docs/` にまとめています。

- **[詰将棋ルール整理](docs/tsume-rules.md)** — 本アプリが採用するルール、合駒・無駄合いの定義と実装状況
- **[生成エンジンの実装解説](docs/generator.md)** — 生成パイプライン、候補局面の作り方、検証・正規化の流れ
- **[生成アルゴリズム調査](docs/generation-algorithms.md)** — ランダム法・逆算法・df-pn の比較、改善方針
- **[df-pn 高速化調査](docs/dfpn-speedup.md)** — n手詰ルーチン、証明駒・反証駒など高速化テクニック

## 詰将棋ルール（概要）

本アプリが採用する詰将棋ルール（詳細は [docs/tsume-rules.md](docs/tsume-rules.md) を参照）:

1. **攻め方は王手の連続で詰ます**
2. **守り方は最善を尽くす**（最長抵抗）
3. **守り方の持ち駒は駒箱**（盤上と攻め方持ち駒以外の全駒）
4. **唯一解**
5. **最短手数**
6. **駒余りなし**
7. **打ち歩詰め・二歩・行き場のない駒の禁止**

> **注**: 無駄合い判定は簡易版を実装（スライド王手に対する合駒→同Xで詰むなら無駄合いとしてスキップ）。

## パズル生成アルゴリズム（概要）

パズルは4つの手法を組み合わせて生成されます（詳細は [docs/generator.md](docs/generator.md) を参照）:

1. **逆算法** — 詰み局面から手を巻き戻して初期局面を構築
2. **延長法** — 短手数のパズルから2手ずつ延長して長手数パズルを生成
3. **ランダム法** — 駒をランダムに配置し、詰将棋として成立するか検証
4. **変異法** — 既存パズルの駒を微小変異させて新パズルを派生

### ソルバー

- **5手詰以下**: 全幅探索（メモ化付きミニマックス）
- **7手詰以上**: df-pn（証明数・反証数探索）+ 段階的バリデーション

### 多様性フィルタ

生成済みパズルの中から、解の手順が類似するパズルを除外して公開用データを作成します。延長法や変異法は同じ詰み筋のバリエーションを大量に生成するため、同一手順のパズルは1問に制限しています。

## GitHub Pages

- `main` ブランチへの push で自動デプロイ（`.github/workflows/pages.yml`）
- リポジトリ設定の `Settings > Pages > Build and deployment` は `GitHub Actions` を選択してください

## ライセンス

MIT (LICENSE 参照)
