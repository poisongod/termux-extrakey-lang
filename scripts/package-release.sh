#!/data/data/com.termux/files/usr/bin/env bash
# 打包 Release 资产：tekl-<arch> + termux.tekl + README.md
#
# 用法:
#   scripts/package-release.sh <arch> [binary]
#     <arch>    aarch64 | armv7
#     [binary]  默认 target/release/tekl
#
# 产物: release/tekl-<arch>.tar.gz
# 上传: gh release upload <tag> release/tekl-<arch>.tar.gz --clobber
set -euo pipefail

arch="${1:?用法: scripts/package-release.sh <arch> [binary]}"
bin="${2:-target/release/tekl}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$root/release"

[ -f "$bin" ] || { echo "找不到二进制: $bin" >&2; exit 1; }
[ -f "$root/termux.tekl" ] || { echo "找不到出厂模板: $root/termux.tekl" >&2; exit 1; }

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

cp "$bin" "$stage/tekl-$arch"
cp "$root/termux.tekl" "$root/README.md" "$stage/"

# 归一化权限：二进制可执行，文档与模板可读
chmod 755 "$stage/tekl-$arch"
chmod 644 "$stage/termux.tekl" "$stage/README.md"

mkdir -p "$out"
tar -C "$stage" -czf "$out/tekl-$arch.tar.gz" "tekl-$arch" termux.tekl README.md

echo "已生成: $out/tekl-$arch.tar.gz"
tar tzvf "$out/tekl-$arch.tar.gz"
echo "上传: gh release upload <tag> release/tekl-$arch.tar.gz --clobber"
