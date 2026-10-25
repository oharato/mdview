# mdview デモ

`mdcat` の書式をそのままに、**画像**と **Mermaid** を端末内に描画します。
テキスト部分は mdcat が担当するので、見た目はいつもの mdcat と同じです。

## テキスト表現

- **太字**、*斜体*、~~打ち消し線~~、`インラインコード`
- リンク: [oharato/mdview](https://github.com/oharato/mdview)
- 入れ子のリスト
  - レベル 2 の項目
    1. さらに深い番号付き
    2. もうひとつ

1. 手順その一
2. 手順その二
   - 補足はこのようにぶら下げる

> 引用はこのように表示されます。
> 複数行にまたがっても大丈夫です。

---

### 表

| プロトコル | 使える端末 | 解像度 |
|---|---|---|
| Kitty graphics | WezTerm / kitty / Ghostty / Herdr | フル |
| Sixel | Windows Terminal 1.22+ / xterm | フル |
| iTerm2 | iTerm2 / VSCode | フル |
| 半ブロック | その他の端末 | 低 |

## Mermaid

### フローチャート

```mermaid
flowchart LR
  A[Markdown] --> B{mdcat}
  B -->|テキスト| C[整形表示]
  B -->|画像 / Mermaid| D[Kitty / Sixel / iTerm2]
  D -->|非対応| E[半ブロック]
```

### シーケンス図

```mermaid
sequenceDiagram
  participant U as ユーザー
  participant M as mdview
  participant T as 端末
  U->>M: mdview README.md
  M->>M: Markdown を分解
  M->>T: mdcat（テキスト）
  M->>T: Kitty / Sixel（画像と図）
  T-->>U: 表示
```

### 円グラフ

```mermaid
pie title 描画プロトコルの内訳
  "Kitty graphics" : 55
  "Sixel" : 20
  "iTerm2" : 10
  "半ブロック" : 15
```

### クラス図

```mermaid
classDiagram
  class Renderer {
    +string name
    +render(image)
  }
  class Kitty
  class Sixel
  Renderer <|-- Kitty
  Renderer <|-- Sixel
```

## 画像

PNG（スクリーンショットや写真など）:

![サンプル画像](sample.png)

SVG（ベクター画像）:

![SVG の例](diagram.svg)

## コード

```typescript
export function hello(name: string): string {
  return `こんにちは, ${name}!`;
}
```

```bash
mdview --check
mdview --update
```

```json
{ "renderer": "kitty", "imageWidth": 109, "light": false }
```

## 日本語と絵文字

全角と半角が混在していても表示幅は正しく計算されます。
絵文字もそのまま: 🎨 📐 🖼️
