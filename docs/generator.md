# generator の実装解説

このディレクトリは、詰将棋パズルの自動生成と検証を行う Rust 製ツール (`tsume-gen`) です。

- エントリポイント: `generator/src/main.rs`
- 生成ロジック: `generator/src/generate.rs`
- 将棋ルール・詰み判定: `generator/src/shogi.rs`

## 1. 使い方と実行フロー

`main.rs` は以下 2 サブコマンドを受けます。

- `generate`: パズル生成（デフォルト）
- `validate`: 既存 JSON の検証

`generate` の主なオプション:

- `--max=`: 各手数で生成する最大問題数（既定 100）
- `--seed=`: 乱数シード
- `--attempts1=`, `--attempts3=`, `--attempts5=`, `--attempts7=`: 手数ごとの試行回数

実行時は `mate_len = [1, 3, 5, 7]` を順に処理し、結果を両方へ出力します。

- `puzzles/{mate_len}.json`
- `public/puzzles/{mate_len}.json`

`validate` は `puzzles/*.json`（なければ `docs/puzzles/*.json`）を読み込み、重複局面を除いて `validate_tsume_puzzle` で成立性を再確認します。

## 2. 生成パイプライン (`generate.rs`)

`generate_puzzles(seed, mate_length, attempts, curated_seeds, max)` が中心です。

1. 収集済み問題（curated）を取り込み
2. その左右反転も取り込み
3. ランダム生成 + 検証を rayon で並列実行
4. 見つかった局面から変異（mutation）を派生
5. 変異フェーズを追加実行
6. 難易度順 + 多様性を考慮して並べ替え

最終的に `Puzzle` 配列へ詰め、`id` を 1 から振り直します。

### 2.1 候補局面の作り方

`random_candidate` は守り方玉の近傍に攻め方・守り方の駒をランダム配置します。手数ごとに `candidate_params` で以下を調整します。

- 駒数レンジ
- 玉周辺の配置レンジ
- 攻め方持ち駒（通常駒/大駒）を持たせる確率

### 2.2 変異 (`mutate_initial`)

既存局面に対してランダム操作を 1 つ適用します。

- 駒移動
- 駒種変更
- 駒追加
- 駒削除
- 攻め方持ち駒の調整

適用後に盤外座標を clamp し、`basic_validity` を満たすものだけ候補に残します。

### 2.3 検証・正規化 (`validate_and_prune`)

候補局面は次の順で整形されます。

1. 攻め方玉を除去（存在しても正規化で削る）
2. `validate_tsume_puzzle` で詰み成立を確認
3. `prune_initial` で不要駒を削減（成立する限り削る）
4. 解を再検証
5. 解再生後に未使用の攻め方持ち駒を初期局面から削除し再検証
6. 守り方玉が左側なら左右反転して右側へ正規化
7. `score_puzzle` を計算

### 2.4 重複・偏り抑制

`add_result` で以下を同時チェックします。

- 完全一致 (`sig_set`)
- 構造シグネチャ一致 (`struct_set`): 守り方玉からの相対配置 + 左右反転正規化
- 駒構成キーごとの出現上限（最大3）
- 攻め方駒構成キーごとの出現上限（最大3）

## 3. 並べ替え戦略（難易度 + 多様性）

生成済み候補には 2 種スコアがあります。

- `score_puzzle`: 面白さ寄り（捨て駒、打ち、成り、着手先の多様さ等）
- `difficulty_score`: 難しさ寄り（持ち駒量、大駒持ち、守備駒数など）

最終順序は `diversify_order` で決定します。

1. 難易度スコアで昇順
2. 上位 `max` 件を 4 分位に分割
3. 各分位内で特徴ベクトル距離（Manhattan 距離）を使って greedy に散らす

このため、全体として「易→難」の流れを保ちつつ、似た問題の連続を減らします。

## 4. 詰み判定ロジック (`shogi.rs`)

### 4.1 局面表現

- 盤: `State.board[81]`
- 持ち駒: `Hands { attacker[7], defender[7] }`
- 手番: `side_to_move`
- 玉位置キャッシュ: `attacker_king`, `defender_king`

`State::set` で玉キャッシュを同期する設計です。

### 4.2 合法手生成

- `pseudo_moves_from`: 駒の擬似合法手（成り分岐込み）
- `pseudo_drops`: 打ち駒候補（行き場なし・二歩を除外）
- `legal_moves`: 自玉が王手になる手を除外、打ち歩詰め禁手も除外

打ち歩詰めは `pawn_drop_mate_forbidden` で、歩打ち後に相手へ合法手が無いかを確認して判定します。

### 4.3 詰み探索

`forced_mate_within(state, plies, memo)` はメモ化付き再帰探索です。

- 守り方手番: 1 手でも逃れがあれば不詰
- 攻め方手番: 王手になる手のみ探索（詰将棋ルール）
- 同一局面 + 残り手数をハッシュ化して再利用
- 成立時は 1 本の手順 `line` と `unique`（唯一性）を返す

### 4.4 問題としての成立条件

`validate_tsume_puzzle` は以下を満たすときのみ `Some(solution)` を返します。

- 手数が奇数
- 攻め方手番
- 守り方玉が存在
- 初期局面で守り方玉に王手がかかっていない
- 指定手数以内で詰む
- 2 手短い手数では詰まない（最短性）
- 解が唯一

## 5. curated データの読み込み

`load_curated("data/curated-puzzles.json")` は JSON の `"3"`, `"5"`, `"7"`, `"9"` キーを読み取り、`HashMap<u32, Vec<InitialData>>` として返します。

現在の `main.rs` は 1/3/5/7 手詰のみ生成対象なので、`"9"` を読んでも通常実行では未使用です。

## 6. 補足

- 乱数は独自 XORShift 系 `Rng` を利用
- 並列化は `rayon::par_iter()` でバッチ単位に実施
- `validate` は局面シグネチャ重複をスキップして検証時間を短縮
