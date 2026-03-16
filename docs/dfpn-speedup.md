# df-pn 検証の高速化

## 現状

7手詰以上のパズル検証に df-pn（`shogi-core/src/dfpn.rs`）を使用しています。段階的バリデーション（`validate_tsume_dfpn_staged`）により、詰み判定→短手数不存在→唯一解チェックを効率的に行います。

### 実装済みの高速化

| テクニック | 効果 | 状態 |
|-----------|------|------|
| 1手詰ルーチン（`is_mate_in_1`） | 小 | 実装済み（depth==1 で使用） |
| 証明駒・反証駒 | 大（24%高速化） | 実装済み |
| 手の並び替え | 小〜中 | 実装済み（捕獲+成り > 捕獲 > 成り > 移動 > 打ち） |
| 無駄合い簡易判定 | 大 | 実装済み（スライド王手→合駒→同Xで詰むならスキップ） |
| 段階的バリデーション | 中 | 実装済み |

### ベンチマーク（validate コマンド）

証明駒・反証駒の導入により、validate の所要時間が 36.7s → 28.0s に改善（24%高速化）。

## 未実装の高速化テクニック

### 3手詰ルーチン

1手詰ルーチンを拡張した3手詰専用ルーチン。テスト実装したが、TT アクセスなしの実装では逆に遅くなったため `#[cfg(test)]` のみに限定。TT と連携した実装であれば効果が見込める。

### 閾値制御の最適化

現在の実装は標準的な df-pn の閾値制御に準拠しており、大きな改善余地は少ない。

```rust
// OR ノード
c_th_pn = th_pn.min(second_pn + 1)
c_th_dn = th_dn - sum_dn + child_dn[best]

// AND ノード
c_th_dn = th_dn.min(second_dn + 1)
c_th_pn = th_pn - sum_pn + child_pn[best]
```

### 探索の並列化

df-pn 内部の並列化は GHI 問題（Graph History Interaction）との兼ね合いで複雑。現在は独立した候補局面の検証を rayon で並列化しており、これが最も安全で効果的なアプローチ。

## 参考文献

- [詰将棋に対するdf-pnアルゴリズムの解説](https://komorinfo.com/blog/df-pn-basics/)
- [証明駒／反証駒の活用方法](https://komorinfo.com/blog/proof-piece-and-disproof-piece/)
- [高速な詰将棋アルゴリズムを完全に理解したい](https://qhapaq.hatenablog.com/entry/2020/07/19/233054)
- [DFPN - minimax.dev](https://minimax.dev/docs/ultimate/pn-search/dfpn/)
- [Proof-Number Search and its Variants (PDF)](https://dke.maastrichtuniversity.nl/m.winands/documents/pnchapter.pdf)
