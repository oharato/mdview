# mdview

`mdcat` の書式をそのままに、**画像と Mermaid を端末内に描画する** Markdown ビューアです。

Sixel 対応の端末（Windows Terminal 1.22+ / WezTerm / xterm など）ではフル解像度で、
非対応の端末では ANSI トゥルーカラーの半ブロック（`▀`）で描画します。
Mermaid は [Pele](https://github.com/obsidianmd/pele) でレンダリングするため、
Chrome / Puppeteer は不要です。

![出力例](docs/diagram.png)

## なぜ必要か

`mdcat` は kitty / iTerm2 / Terminology 系の画像プロトコルしか話せません。
Windows Terminal や SSH 越しの端末では画像も Mermaid も表示されず、
Mermaid はコードブロックのまま流れてしまいます。

`mdview` は

1. テキスト部分を `mdcat` に渡し（見た目は mdcat と同一）、
2. Mermaid を Pele で SVG → resvg で PNG 化し、
3. 画像・図を端末の実ピクセルに合わせて Sixel（または半ブロック）で描画します。

## 特徴

- **mdcat 互換のテキスト表示** — シンタックスハイライト、リンク、見出し記号などは mdcat のまま
- **Mermaid を内蔵レンダリング** — Pele（ゼロ DOM）+ resvg。1 図あたり 0.1〜0.2 秒、Chrome 不要
- **Sixel を自前実装** — 外部の画像ビューア不要（`img2sixel` / `chafa` などは不要）
- **表示サイズを自動補正** — `CSI 16t` / `CSI 14t` で端末のセルサイズ・ウィンドウサイズを実測し、
  HiDPI（150% / 200% スケーリング）でも指定した桁数ぴったりに表示
- **自動フォールバック** — 非対応端末では半ブロック描画、Pele が無ければ mermaid-cli（任意）
- **キャッシュ** — 図・画像は `~/.cache/mdview/` に保存され、2 回目以降は即表示
- **TypeScript / ESM** — Node.js LTS だけで動作。Python は不要

## 必要なもの

| 依存 | 用途 | 必須 |
|---|---|---|
| [Node.js](https://nodejs.org/) 22.12+（LTS） | 実行環境 | 必須 |
| [mdcat](https://github.com/swsnr/mdcat) | Markdown のテキスト描画 | 必須 |
| [pele](https://www.npmjs.com/package/pele) | Mermaid の SVG 化 | 同梱（依存関係） |
| [@resvg/resvg-js](https://www.npmjs.com/package/@resvg/resvg-js) | SVG → PNG | 同梱（依存関係） |
| [sharp](https://sharp.pixelplumbing.com/) | 画像のデコード・リサイズ | 同梱（依存関係） |
| pnpm 10+ | インストール / ビルド | 必須 |
| [mermaid-cli](https://github.com/mermaid-js/mermaid-cli) | Pele が使えないときの代替 | 任意 |

## インストール

```bash
git clone https://github.com/oharato/mdview.git
cd mdview
./install.sh
```

`install.sh` は次を行います。

- Node.js / pnpm の確認（pnpm は corepack で有効化を試みます）
- `mdcat` が無ければ GitHub Releases から `~/.local/bin` に取得
- `pnpm install` → `pnpm build`（TypeScript → `dist/`）
- `~/.local/bin/mdview` から `dist/cli.js` へシンボリックリンク

インストール先を変えたい場合は `BIN_DIR=~/bin ./install.sh` のように指定してください。

### pnpm のグローバルインストール

```bash
pnpm add -g github:oharato/mdview   # prepare でビルドされます
```

## 使い方

```bash
mdview README.md                      # 表示
cat README.md | mdview                # 標準入力
mdview -w 100 -i 100 doc.md           # テキスト100桁 / 画像100桁
mdview -r blocks doc.md               # Sixel を使わず半ブロックで描画
mdview --check                        # 端末の対応状況を表示
mdview --refresh doc.md               # 端末判定をやり直す
```

### オプション

| オプション | 説明 |
|---|---|
| `-w, --width N` | テキストの最大桁数（既定: 端末幅、最大 120） |
| `-i, --image-width N` | 画像・図の横幅（端末桁数、既定: 端末幅いっぱい） |
| `-r, --renderer auto\|sixel\|blocks` | 描画方式（既定: 端末を自動判定） |
| `--image-colours N` | Sixel の色数（既定: 256） |
| `--image-bg auto\|none\|#RRGGBB` | 透過画像の背景（既定: 端末に合わせる） |
| `--pixel-scale F` | セルサイズを取得できない端末での倍率（HiDPI なら 2） |
| `--upscale / --no-upscale` | 小さい画像を端末幅まで拡大（既定: 有効・最大 3 倍） |
| `--light` | 明るい背景の端末として扱う（Mermaid のテーマ・画像背景） |
| `--mermaid-engine auto\|pele\|mmdc` | Mermaid の描画エンジン（既定: Pele） |
| `--mermaid-theme auto\|default\|dark\|neutral` | Mermaid の配色 |
| `--mermaid-width N` | Mermaid のラスタライズ幅 px（既定: 2048） |
| `-c, --no-colour` | 色・画像を無効化（生の Markdown に近い表示） |
| `--check` | 端末・依存の対応状況を表示して終了 |

環境変数: `MDVIEW_SIXEL=0|1`（Sixel の強制）、`MDVIEW_CELL=18x36`（セルサイズ上書き）、
`MDVIEW_CACHE`（キャッシュ先）、`MDVIEW_MDCAT`（mdcat のパス）。

## 動作のしくみ

1. **端末プローブ** — DA1（`ESC [ c`）で Sixel 対応を、`CSI 16 t` / `CSI 14 t` で
   セルサイズ・ウィンドウサイズ（実ピクセル）を取得。判定結果は `~/.cache/mdview/caps.json` に保存。
2. **Markdown の分解** — ```` ```mermaid ```` フェンスと `![](...)` をプレースホルダに置換し、
   テキスト部分だけ `mdcat --ansi` に渡す。
3. **Mermaid** — Pele が出力する SVG は CSS 変数で配色されるため、変数を実色へ解決してから
   resvg でラスタライズ（resvg は `var()` 非対応）。ダイアグラムは内容のハッシュでキャッシュ。
4. **Sixel エンコード** — メディアンカットで減色し、6 ピクセルバンド + `!<n><char>` の
   ランレングス圧縮で出力。1 図あたり数十 KB。
5. **半ブロック描画** — Sixel 非対応端末では `▀` と 24bit カラーで描画（`--colours 256` で 256 色）。

### 開発

```bash
pnpm install
pnpm test        # vitest（Sixel エンコーダ / Markdown 分解 / テーマ解決）
pnpm typecheck
pnpm build
```

## 制限

- Sixel は **Windows Terminal 1.22 以降**で利用できます。tmux / screen 経由では
  パススルー設定がない限り表示されません（その場合は自動で半ブロックになります）。
- `less` などのページャと併用すると画像は出ません（mdcat 単体と同じ制約）。
- 半ブロック描画は 1 セル 2 ピクセル相当のため、文字を含む複雑な図は粗くなります。
  Sixel 対応端末での利用を推奨します。
- Mermaid の対応範囲は Pele に準じます（Mermaid 12.1 相当）。描画できない記法は
  コードブロックとしてそのまま表示されます。

## ライセンス

MIT
