#!/bin/bash
set -euo pipefail

echo "===设置开发环境==="

check_deps() {
	local deps=(git curl make gcc clang python3 bc bison flex cpio gzip qemu-system-x86_64)
	local missing=()

	for dep in "${deps[@]}"; do
		if ! command -v "$dep" >/dev/null 2>&1; then
			missing+=("$dep")
		fi
	done

	if [ ${#missing[@]} -gt 0 ]; then
		echo "缺少依赖：${missing[*]}"
		exit 1
	fi
	echo "依赖检查通过"
}

check_deps

if [ ! -d "linux/.git" ]; then
	echo "下载linux内核..."
	git clone https://github.com/Rust-for-Linux/linux.git linux
else
	echo "Linux内核已存在"
fi

if [ ! -d "busybox/.git"]; then
	echo "下载Busybox..."
	git clone https://github.com/mirror/busybox.git busybox
else
	echo "Busybox已存在"
fi

if ! command -v rustup >/dev/null 2>&1; then
	echo "安装Rust..."
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
	source "$HOME/.cargo/env"
fi

echo "设置Rust版本..."
rustup override set stable
rustup component add rust-src rustfmt  clippy

echo "配置内核..."
ln -sf "../qemu-busybox-min.config" "linux/kernel/configs/qemu-busybos-min.config"

cd linux
echo "检查Rust支持..."
make LLVM=1 CLIPPY=1 rustavailable

echo "应用配置..."
yes "" | make LLVM=1 CLIPPY=1 defconfig qemu-busybox-min.config rust.config 2>/dev/null | tail -20 || true
yes "" | make LLVM=1 CLIPPY=1 olddefconfig 2>/dev/null | tail -20 || true

echo "生成Rust分析配置..."
make  LLVM=1 CLIPPY=1 rust-analyzer
cd ..

echo "配置Busybox..."
cd busybox
yes  "" | make defconfig 2>/dev/null | tail -10 || true
sed -i 's/.*CONFIG_STATIC.*/CONFIG_STATIC=y/' .config
sed -i 's/.*CONFIG_STATIC_LIBGCC.*/CONFIG_STATIC_LIBGCC=y/'  .config
sed -i 's/.*CONFIG_TC.*/CONFIG_TC=n/' .config
yes "" | make oldconfig 2>/dev/null | tail -10 || true
cd ..

echo ""
echo "设置完成"
echo "可运行make build"
