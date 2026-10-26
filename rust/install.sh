#!/usr/bin/env bash
# Install the mdview Rust single binary from GitHub Releases.
#
#   ./rust/install.sh
#   BIN_DIR=~/bin ./rust/install.sh
#   MDVIEW_VERSION=v0.7.0 ./rust/install.sh
set -euo pipefail

REPO="oharato/mdview"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[!]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[x]\033[0m %s\n' "$*" >&2; exit 1; }

case "$(uname -m)" in
  x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
  aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
  *) die "未対応のアーキテクチャです: $(uname -m)（x86_64 / aarch64 のみ）" ;;
esac

if [ -n "${MDVIEW_VERSION:-}" ]; then
  case "$MDVIEW_VERSION" in
    v*) ;;
    *) MDVIEW_VERSION="v${MDVIEW_VERSION}" ;;
  esac
  base="https://github.com/${REPO}/releases/download/${MDVIEW_VERSION}"
  version="${MDVIEW_VERSION}"
else
  base="https://github.com/${REPO}/releases/latest/download"
  version="latest"
fi

command -v curl >/dev/null 2>&1 || die "curl が必要です"
command -v sha256sum >/dev/null 2>&1 || die "sha256sum が必要です"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

info "mdview (${version}, ${target}) を取得します"
curl -sSLf "${base}/mdview-${target}" -o "${tmp}/mdview" \
  || die "バイナリをダウンロードできませんでした（リリースが存在するか確認してください）"
curl -sSLf "${base}/SHA256SUMS" -o "${tmp}/SHA256SUMS" \
  || warn "SHA256SUMS を取得できませんでした（チェックサム検証を省略します）"

if [ -s "${tmp}/SHA256SUMS" ]; then
  expected=$(awk -v f="mdview-${target}" '$2 == f { print $1 }' "${tmp}/SHA256SUMS")
  actual=$(sha256sum "${tmp}/mdview" | awk '{ print $1 }')
  if [ -n "$expected" ]; then
    [ "$expected" = "$actual" ] || die "チェックサムが一致しません"
    info "チェックサム OK (SHA-256)"
  else
    warn "SHA256SUMS に mdview-${target} の記載がありません"
  fi
fi

mkdir -p "$BIN_DIR"
chmod +x "${tmp}/mdview"
mv -f "${tmp}/mdview" "$BIN_DIR/mdview"
info "mdview -> $BIN_DIR/mdview"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) warn "$BIN_DIR が PATH にありません: export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac

info "完了。確認: mdview --version && mdview --check"
