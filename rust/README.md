# mdview (Rust single binary)

`mdcat` の書式をそのままに、画像と Mermaid を端末内に描画する Markdown ビューアの
**Rust 実装**です。Node.js / npm / sharp / libvips / 外部 mdcat プロセスを必要とせず、
Linux 向けの単体バイナリとして配布できます。

- テキスト描画: [`pulldown-cmark-mdcat`](https://crates.io/crates/pulldown-cmark-mdcat)（mdcat 本体のレンダラ）
- Mermaid: [`mermaid-rs-renderer`](https://crates.io/crates/mermaid-rs-renderer)（Pure Rust）
- 画像: [`image`](https://crates.io/crates/image) + [`resvg`](https://crates.io/crates/resvg)
- 端末: Kitty graphics / Sixel / iTerm2 / ANSI 半ブロック

> このディレクトリは `poc/rust-single-binary` ブランチで追加した実装です。
> 従来の Node.js 実装はリポジトリ直下の `src/` に残しています。

## 特徴

- **単体バイナリ** — Node.js も Python も外部 `mdcat` も不要。release ビルドは約 12MB
- **mdcat 互換のテキスト** — `pulldown-cmark-mdcat 2.9.0` を使用。
  デモでは `mdcat 2.7.1 --ansi --columns 100` の出力とバイト単位で一致。
  （2.7.1 は crates.io で yanked、2.10 以降は別メンテナのフォークのため 2.9.0 に固定）
- **CJK 対応の表** — mdcat の表レンダラはバイト幅で列を計算するため、日本語・絵文字が
  ずれる。mdview は East Asian Width を考慮して自前で描画
- **Mermaid を内蔵** — `mermaid-rs-renderer` で SVG を作り、`resvg` で PNG 化。
  Chrome / Puppeteer / Pele は不要
- **画像プロトコルを内蔵** — Kitty graphics / Sixel / iTerm2 / 半ブロック。
  `img2sixel` / `chafa` / `kitten icat` などの外部ツールは不要
- **透過を保持** — Kitty / iTerm2 / 半ブロックでは透過 PNG・SVG が端末背景に溶け込む。
  Sixel は仕様上アルファを持てないため背景色に合成
- **端末の自動判定** — DA1 / CSI 16t / CSI 14t / XTVERSION / Kitty クエリ・転送テスト
- **ページャ** — 前後移動・見出し一覧・セクションジャンプ。画像もそのまま表示
- **キャッシュ** — Mermaid とリモート画像を `~/.cache/mdview/` に保存
- **自己更新** — GitHub Releases の Linux バイナリを取得して置き換え

## 対応環境

| 項目 | 対応 |
|---|---|
| OS | Linux（x86_64 / aarch64） |
| 端末 | WezTerm / kitty / Ghostty / Herdr / Windows Terminal (WSL) / xterm / iTerm2 / VSCode |
| 画像形式 | PNG / JPEG / GIF / WebP / BMP / TIFF / SVG |
| Mermaid | `mermaid-rs-renderer` が対応する 23 種類 |

Windows ネイティブは未対応です。端末プローブが `/dev/tty` を使うため、WSL 経由で
利用してください（ページャ自体は crossterm ベースです）。

## ビルド

Rust 1.99 以降が必要です。mise を使う場合:

```bash
mise use -g rust@1.99.1

git clone https://github.com/oharato/mdview.git
cd mdview/rust
cargo build --release
./target/release/mdview --version
```

`cargo build --release` は既定でサイズ最適化（`opt-level = "z"`, LTO, strip）されます。
バイナリは `rust/target/release/mdview` にできます。

### ローカルインストール

```bash
# リポジトリルートで
cargo install --path rust

# または rust/ 内で
cd rust && cargo install --path .

# GitHub Releases から（リリース後）
./rust/install.sh
```

`rust/install.sh` は GitHub Releases の最新 Linux バイナリを `~/.local/bin` に
インストールします（リリース後）。

## 使い方

```bash
mdview examples/demo.md               # 表示
mdview -w 100 -i 80 examples/demo.md  # テキスト100桁 / 画像80桁
mdview -r blocks examples/demo.md     # 画像プロトコルを使わず半ブロック
mdview -p README.md                   # 内蔵ページャ
mdview --toc README.md                # 見出し一覧
mdview --section 3 README.md          # 3 番目の見出しのセクション
mdview --check                        # 端末・ビルド情報
mdview --refresh examples/demo.md     # 端末判定をやり直す
mdview --check-update                 # 最新リリースとの比較
mdview --update                       # 自分自身を更新
```

標準入力にも対応しています。

```bash
cat README.md | mdview
```

### オプション

| オプション | 説明 |
|---|---|
| `-w, --width N` | テキストの最大桁数（既定: 端末幅、最大 120） |
| `-i, --image-width N` | 画像・図の横幅（端末桁数、既定: 端末幅 - 2） |
| `-r, --renderer auto\|kitty\|sixel\|iterm\|blocks` | 描画方式（既定: 自動） |
| `--colours truecolor\|256` | 半ブロック描画時の色数 |
| `--image-colours N` | Sixel の色数（既定: 256、2〜256） |
| `--image-bg auto\|none\|#RRGGBB` | 透過画像の扱い。`auto` はアルファ対応方式なら透過を保持し、Sixel は背景色に合成 |
| `--pixel-scale F` | セルサイズを取得できない端末での倍率（HiDPI なら 2） |
| `--upscale / --no-upscale` | 小さい画像を端末幅まで拡大（既定: 有効・最大 3 倍） |
| `--light` | 明るい背景の端末として扱う |
| `-c, --no-colour` | 色・画像を無効化 |
| `--mermaid-theme auto\|dark\|default\|neutral\|modern` | Mermaid の配色（既定: auto） |
| `--mermaid-width N` | Mermaid のラスタライズ幅 px（既定: 2048） |
| `-p, --paginate` | 内蔵ページャで表示 |
| `--toc` | 見出し一覧 |
| `--section N` | N 番目の見出しのセクションだけ表示 |
| `--check` | 端末・ビルド情報を表示 |
| `--refresh` | 端末判定をやり直す（caps キャッシュを無視） |
| `--check-update` | 新しいリリースがあるか確認 |
| `--update` | 最新リリースへ自己更新（Linux） |
| `-V, --version` | バージョン |
| `-h, --help` | ヘルプ |

### 環境変数

| 変数 | 説明 |
|---|---|
| `MDVIEW_KITTY=0\|1` | Kitty graphics を強制 |
| `MDVIEW_SIXEL=0\|1` | Sixel を強制 |
| `MDVIEW_CELL=18x36` | セルサイズを上書き |
| `MDVIEW_CACHE=DIR` | キャッシュ先（既定: `~/.cache/mdview`） |

### ページャのキー

| キー | 動作 |
|---|---|
| `space` / `Enter` / `j` / `↓` | 次のページ |
| `b` / `k` / `↑` | 前のページ |
| `]` / `[` | 次の見出し / 前の見出し |
| `t` | 見出し一覧（`j`/`k` で選択、`Enter` でジャンプ、`t`/`Esc` で戻る） |
| `g` / `G` | 先頭 / 末尾 |
| `q` / `Esc` / `Ctrl-C` | 終了 |

## 動作のしくみ

```text
Markdown
  ├─ Mermaid フェンス / 単独行画像 / GFM 表を抽出
  ├─ テキスト部分 → pulldown-cmark-mdcat → ANSI
  ├─ 表 → 自前レンダラ（East Asian Width）
  ├─ Mermaid → mermaid-rs-renderer → SVG → resvg → PNG
  └─ 画像 → image / resvg → PNG / RGBA
        └─ Kitty graphics / Sixel / iTerm2 / 半ブロック
```

主なモジュール:

| ファイル | 役割 |
|---|---|
| `src/main.rs` | CLI、文書処理、描画の振り分け |
| `src/text.rs` | `pulldown-cmark-mdcat` によるテキスト描画 |
| `src/markdown.rs` | Mermaid・画像の抽出とセグメント分割 |
| `src/tables.rs` | CJK 幅対応の表抽出・描画 |
| `src/mermaid.rs` | Mermaid → SVG → PNG、テーマ、キャッシュ |
| `src/images.rs` | 画像読み込み・リサイズ・SVG ラスタライズ・背景合成 |
| `src/blocks.rs` | ANSI 半ブロック描画 |
| `src/sixel.rs` | メディアンカット減色 + Sixel エンコーダ |
| `src/kitty.rs` | Kitty graphics エンコーダ |
| `src/iterm.rs` | iTerm2 inline image エンコーダ |
| `src/term.rs` | 端末プローブ、能力判定、キャッシュ |
| `src/selftest.rs` | Kitty の画像転送セルフテスト |
| `src/pager.rs` | ページャ、見出しジャンプ |
| `src/remote.rs` | リモート画像の取得とキャッシュ |
| `src/update.rs` | リリース確認・自己更新 |

## テスト

```bash
cd rust
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

テストは以下をカバーします。

- Sixel の減色・RLE・実デコード
- Kitty / iTerm2 のエスケープ列
- Markdown の Mermaid・画像・表の抽出
- CJK / 絵文字の表示幅、表の整列・切り詰め
- 見出し・セクション抽出
- ページャのページ分割
- 端末応答のパース
- Mermaid の SVG / PNG / キャッシュ
- CLI の end-to-end（`tests/cli.rs`）

`mdcat` とのテキスト一致を確認するには、同じ桁数を指定してください。

```bash
mdview --dump-text -w 100 examples/demo.md > /tmp/rs.txt
mdcat --ansi --columns 100 examples/demo.md > /tmp/mdcat.txt
diff -u /tmp/mdcat.txt /tmp/rs.txt
```

`--dump-text` はデバッグ用の隠しオプションです。

## リリース

`.github/workflows/rust.yml` が fmt / clippy / test / release build を実行します。
タグ `v*` を push すると `.github/workflows/rust-release.yml` が Linux バイナリを作成し、
以下の asset を GitHub Releases に添付します。

- `mdview-x86_64-unknown-linux-gnu`
- `mdview-aarch64-unknown-linux-gnu`
- `SHA256SUMS`

`mdview --update` はこの asset をダウンロードし、`SHA256SUMS` を検証して
実行中のバイナリを置き換えます。

## 既知の制限・Node 版との違い

- **Mermaid の見た目**: `mermaid-rs-renderer 0.3.1` は開発初期の Pure Rust 実装です。
  flowchart / sequence / pie は実用的ですが、class diagram のレイアウトは Pele と
  一致しません。Mermaid の完全な再現が必要な場合は、Pele を QuickJS 等に埋め込む
  方式に切り替える必要があります。
- **画像形式**: `sharp`/libvips ほど多くの形式は扱えません。現在は PNG / JPEG / GIF /
  WebP / BMP / TIFF / SVG です。AVIF / HEIC などは未対応です。
- **Linux のみ**: 端末プローブが `/dev/tty` と termios を使うため、Windows ネイティブは
  未対応です。
- **`MDVIEW_MDCAT` は不要**: mdcat はライブラリとして組み込まれています。
  外部バイナリのバージョン確認（`--check` の mdcat 行）もありません。
- **表のインライン記法**: 太字・斜体・打ち消し線・コード・リンクに対応。
  折り返しはせず、幅に収まらない場合は `…` で切り詰めます。
- **Sixel**: Windows Terminal 1.22+ / xterm など。WezTerm は DA1 で Sixel 対応を
  返しますが実装していないため、WezTerm では Sixel を使いません。
- **tmux / screen**: パススルー設定がない限り画像プロトコルは通りません。
  その場合は半ブロックにフォールバックします。

## ライセンス

MIT。依存クレートのライセンスは `cargo tree` / `cargo deny` などで確認してください。
`pulldown-cmark-mdcat` は MPL-2.0 AND Apache-2.0、`resvg` は MPL-2.0 です。
