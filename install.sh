#!/bin/sh
# Installs the latest figure release: curl -fsSL https://raw.githubusercontent.com/rafalfigura/figure/main/install.sh | sh
# Env: FIGURE_VERSION=v0.1.0 pins a version; FIGURE_INSTALL_DIR (default ~/.local/bin) picks the directory.
set -eu

repo="rafalfigura/figure"
dir="${FIGURE_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Darwin-arm64) target=aarch64-apple-darwin ;;
  *) echo "figure: no prebuilt binary for $(uname -s) $(uname -m); use: cargo install --git https://github.com/$repo" >&2; exit 1 ;;
esac

if [ -n "${FIGURE_VERSION:-}" ]; then
  base="https://github.com/$repo/releases/download/$FIGURE_VERSION"
else
  base="https://github.com/$repo/releases/latest/download"
fi
name="figure-$target"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "figure: downloading $name"
curl -fsSL "$base/$name.tar.gz" -o "$tmp/$name.tar.gz"
curl -fsSL "$base/$name.tar.gz.sha256" -o "$tmp/$name.tar.gz.sha256"

cd "$tmp"
if command -v sha256sum >/dev/null 2>&1; then sha256sum -c "$name.tar.gz.sha256" >/dev/null
else shasum -a 256 -c "$name.tar.gz.sha256" >/dev/null; fi

tar xzf "$name.tar.gz"
mkdir -p "$dir"
install -m 755 "$name/figure" "$dir/figure"
echo "figure: installed $("$dir/figure" --version) to $dir/figure"

case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "figure: $dir is not on your PATH; add it to your shell profile" ;;
esac
