#!/usr/bin/env sh
# 把所有示例插件编译到 wasm32-unknown-unknown，并把产物拷到各自 manifest.toml 旁（plugin.wasm）。
# 前置：rustup target add wasm32-unknown-unknown
# server 的 wasm 夹具测试与 e2e 依赖这些 plugin.wasm；未构建时相关测试自动跳过。
set -e
cd "$(dirname "$0")"

for dir in trim-trailing testbed webdav-storage s3-storage ai-polish; do
	[ -d "$dir" ] || continue
	echo "==> $dir"
	(cd "$dir" && cargo build --release --target wasm32-unknown-unknown --quiet)
	name=$(echo "$dir" | tr '-' '_')
	cp "$dir/target/wasm32-unknown-unknown/release/$name.wasm" "$dir/plugin.wasm"
done
echo "done."
