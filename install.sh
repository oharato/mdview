#!/usr/bin/env bash
# mdview installer
#
#   ./install.sh              # ~/.local/bin にインストール
#   BIN_DIR=~/bin ./install.sh
#
# 足りない依存（mdcat / resvg / Pele）を自動で取得します。
# Python 3 + Pillow、Node.js が必要です。
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
ARCH="$(uname -m)"

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[!]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[x]\033[0m %s\n' "$*" >&2; exit 1; }

mkdir -p "$BIN_DIR"

# ---- Python / Pillow -------------------------------------------------------
if ! command -v python3 >/dev/null 2>&1; then
  die "python3 が見つかりません"
fi
if ! python3 -c 'import PIL' >/dev/null 2>&1; then
  die "Pillow が必要です: sudo apt install python3-pil  もしくは  pip install pillow"
fi
info "python3 $(python3 -c 'import sys; print("%d.%d" % sys.version_info[:2])') + Pillow OK"

# ---- mdcat ----------------------------------------------------------------
if command -v mdcat >/dev/null 2>&1; then
  info "mdcat $(command -v mdcat)"
else
  if [ "$ARCH" != "x86_64" ]; then
    warn "mdcat のプリビルドは x86_64 のみです (cargo install mdcat をお使いください)"
  else
    info "mdcat を取得します"
    url=$(curl -sSL https://api.github.com/repos/swsnr/mdcat/releases/latest \
          | grep -o 'https://[^"]*x86_64-unknown-linux-gnu.tar.gz' | head -1)
    [ -n "$url" ] || die "mdcat の配布 URL を取得できませんでした"
    tmp=$(mktemp -d)
    curl -sSL "$url" -o "$tmp/mdcat.tar.gz"
    tar xzf "$tmp/mdcat.tar.gz" -C "$tmp"
    install -m 755 "$tmp"/*/mdcat "$BIN_DIR/mdcat"
    rm -rf "$tmp"
    info "mdcat -> $BIN_DIR/mdcat"
  fi
fi

# ---- resvg（Mermaid の SVG ラスタライズ）------------------------------------
if command -v resvg >/dev/null 2>&1 || [ -x "$BIN_DIR/resvg" ]; then
  info "resvg OK"
else
  case "$ARCH" in
    x86_64)  resvg_arch=x86_64 ;;
    aarch64) resvg_arch=aarch64 ;;
    *)       resvg_arch="" ;;
  esac
  if [ -z "$resvg_arch" ]; then
    warn "resvg のプリビルドが無いアーキテクチャです ($ARCH)"
  else
    info "resvg を取得します"
    url=$(curl -sSL https://api.github.com/repos/linebender/resvg/releases/latest \
          | grep -o "https://[^\"]*resvg-linux-${resvg_arch}.tar.gz" | head -1)
    [ -n "$url" ] || die "resvg の配布 URL を取得できませんでした"
    tmp=$(mktemp -d)
    curl -sSL "$url" -o "$tmp/resvg.tar.gz"
    tar xzf "$tmp/resvg.tar.gz" -C "$tmp"
    install -m 755 "$tmp/resvg" "$BIN_DIR/resvg"
    rm -rf "$tmp"
    info "resvg -> $BIN_DIR/resvg"
  fi
fi

# ---- Pele（Mermaid レンダラ）-----------------------------------------------
if command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1; then
  if [ ! -d "$REPO_DIR/pele/node_modules/pele" ]; then
    info "Pele をインストールします"
    (cd "$REPO_DIR/pele" && npm install --silent --no-fund --no-audit)
  fi
  info "Pele $(node -e "console.log(require('$REPO_DIR/pele/node_modules/pele/package.json').version)" 2>/dev/null || echo OK)"
else
  warn "node/npm が見つかりません。Mermaid は描画されません（コードブロックとして表示）"
fi

# ---- mdview 本体 -----------------------------------------------------------
target="$BIN_DIR/mdview"
if [ -e "$target" ] && [ ! -L "$target" ]; then
  mv "$target" "$target.bak.$(date +%Y%m%d%H%M%S)"
  warn "既存の $target をバックアップしました"
fi
ln -sfn "$REPO_DIR/mdview" "$target"
chmod +x "$REPO_DIR/mdview"
info "mdview -> $target"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) warn "$BIN_DIR が PATH にありません: export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac

info "インストール完了。確認: mdview --check && mdview $REPO_DIR/examples/demo.md"
