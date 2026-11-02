#!/data/data/com.termux/files/usr/bin/env bash
# 打包 Release 资产：tekl-<arch> + 出厂模板 + README.md
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

# 出厂模板：首次运行时 provision_user_dir() 从二进制同目录复制到 ~/.termux/
templates=(termux.tekl tekl.ttoll editor.ttoll)

[ -f "$bin" ] || { echo "找不到二进制: $bin" >&2; exit 1; }
for f in "${templates[@]}"; do
    [ -f "$root/$f" ] || { echo "找不到出厂模板: $root/$f" >&2; exit 1; }
done

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

cp "$bin" "$stage/tekl-$arch"
cp "$root/README.md" "$stage/"
for f in "${templates[@]}"; do
    cp "$root/$f" "$stage/"
done

# 归一化权限：二进制可执行，文档与模板可读
chmod 755 "$stage/tekl-$arch"
chmod 644 "$stage/README.md" "${templates[@]/#/$stage/}"

mkdir -p "$out"
tar -C "$stage" -czf "$out/tekl-$arch.tar.gz" "tekl-$arch" README.md "${templates[@]}"

echo "已生成: $out/tekl-$arch.tar.gz"
tar tzvf "$out/tekl-$arch.tar.gz"
echo "上传: gh release upload <tag> release/tekl-$arch.tar.gz --clobber"
