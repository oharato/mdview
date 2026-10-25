#!/usr/bin/env bash
# mdview installer
#
#   ./install.sh                 # ~/.local/bin にインストール（ビルド込み）
#   BIN_DIR=~/bin ./install.sh
#
# Node.js (LTS) と pnpm が必要です。mdcat が無ければ mdcat.version の
# バージョンを取得します（MDCAT_VERSION=x.y.z で上書き、latest で最新）。
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
PINNED_MDCAT=$(tr -d '[:space:]' < "$REPO_DIR/mdcat.version" 2>/dev/null || true)
MDCAT_VERSION="${MDCAT_VERSION:-$PINNED_MDCAT}"

if [ "$MDCAT_VERSION" = "latest" ] || [ -z "$MDCAT_VERSION" ]; then
  MDCAT_VERSION=$(curl -sSL https://api.github.com/repos/swsnr/mdcat/releases/latest \
    | grep -o '"tag_name": *"mdcat-[^"]*"' | sed 's/.*mdcat-//; s/"$//')
  [ -n "$MDCAT_VERSION" ] && info "mdcat の最新版: $MDCAT_VERSION"
fi

install_mdcat() {
  if [ "$ARCH" != "x86_64" ]; then
    warn "mdcat のプリビルドは x86_64 のみです (cargo install mdcat --version $MDCAT_VERSION)"
    return
  fi

  local base asset tmp expected actual
  asset="mdcat-${MDCAT_VERSION}-x86_64-unknown-linux-gnu.tar.gz"
  base="https://github.com/swsnr/mdcat/releases/download/mdcat-${MDCAT_VERSION}"

  info "mdcat ${MDCAT_VERSION} を取得します"
  tmp=$(mktemp -d)
  curl -sSLf "$base/$asset" -o "$tmp/$asset" || die "mdcat ${MDCAT_VERSION} をダウンロードできませんでした"

  # チェックサム（BLAKE2b-512）を検証する
  if command -v b2sum >/dev/null 2>&1; then
    expected=$(curl -sSLf "$base/B2SUMS.txt" 2>/dev/null | awk -v f="$asset" '$2 == f { print $1 }' || true)
    actual=$(b2sum "$tmp/$asset" | awk '{ print $1 }')
    if [ -n "$expected" ]; then
      [ "$expected" = "$actual" ] || die "mdcat のチェックサムが一致しません"
      info "チェックサム OK (BLAKE2b-512)"
    else
      warn "B2SUMS.txt に $asset の記載がないため、チェックサム検証を省略します"
    fi
  else
    warn "b2sum が無いため、チェックサム検証を省略します"
  fi

  tar xzf "$tmp/$asset" -C "$tmp"
  install -m 755 "$tmp"/*/mdcat "$BIN_DIR/mdcat"
  rm -rf "$tmp"
  info "mdcat -> $BIN_DIR/mdcat ($MDCAT_VERSION)"
}

if [ "${MDCAT_FORCE:-0}" = "1" ]; then
  install_mdcat
elif command -v mdcat >/dev/null 2>&1; then
  current=$(mdcat --version 2>/dev/null | head -1 | awk '{ print $2 }')
  info "mdcat $(command -v mdcat) (${current:-unknown}, 推奨 $PINNED_MDCAT)"
  if [ -n "$current" ] && [ "$current" != "$MDCAT_VERSION" ]; then
    info "mdcat を $MDCAT_VERSION に更新します (MDCAT_FORCE=1 相当)"
    install_mdcat
  fi
else
  install_mdcat
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
