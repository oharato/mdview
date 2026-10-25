#!/usr/bin/env bash
# mdview installer
#
#   ./install.sh                 # ~/.local/bin にインストール（ビルド込み）
#   BIN_DIR=~/bin ./install.sh
#
# Node.js (LTS) と pnpm が必要です。mdcat が無ければ自動で取得します。
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
ARCH="$(uname -m)"

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[!]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[x]\033[0m %s\n' "$*" >&2; exit 1; }

mkdir -p "$BIN_DIR"

# ---- Node.js / pnpm --------------------------------------------------------
command -v node >/dev/null 2>&1 || die "Node.js (>=22.12) が必要です: https://nodejs.org/"
node_major=$(node -p 'process.versions.node.split(".")[0]')
[ "$node_major" -ge 22 ] || die "Node.js 22.12 以上が必要です (現在: $(node -v))"
info "node $(node -v)"

if ! command -v pnpm >/dev/null 2>&1; then
  if command -v corepack >/dev/null 2>&1; then
    info "corepack で pnpm を有効化します"
    corepack enable pnpm
  else
    die "pnpm が見つかりません: npm install -g pnpm"
  fi
fi
info "pnpm $(pnpm -v)"

# ---- mdcat -----------------------------------------------------------------
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

# ---- 依存関係のインストールとビルド ----------------------------------------
info "依存関係をインストールします"
(cd "$REPO_DIR" && pnpm install --frozen-lockfile)

info "TypeScript をビルドします"
(cd "$REPO_DIR" && pnpm build)

[ -f "$REPO_DIR/dist/cli.js" ] || die "ビルドに失敗しました (dist/cli.js がありません)"

# ---- mdview 本体 -----------------------------------------------------------
target="$BIN_DIR/mdview"
if [ -e "$target" ] && [ ! -L "$target" ]; then
  mv "$target" "$target.bak.$(date +%Y%m%d%H%M%S)"
  warn "既存の $target をバックアップしました"
fi
chmod +x "$REPO_DIR/dist/cli.js"
ln -sfn "$REPO_DIR/dist/cli.js" "$target"
info "mdview -> $target"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) warn "$BIN_DIR が PATH にありません: export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac

info "インストール完了。確認: mdview --check && mdview $REPO_DIR/examples/demo.md"
