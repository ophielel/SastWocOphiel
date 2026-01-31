#!/bin/bash

set -euo pipefail

usage() {
	echo "Usage: $0 [-k /path/to/linux] [-b /path/to/busybox]"
}

# Are the command installed?
instaled() {
	command -v "$1" >/dev/null 2>&1
}

check_dependencies() {
	local missing=()
	local deps=(git curl make gcc clang ld.lld python3 bc bison flex pkg-config cpio gzip bindgen qemu-system-x86_64)

	for dep in "${deps[@]}"; do
		if ! instaled "$dep"; then
			missing+=("$dep")
		fi
	done

	if ((${#missing[@]})); then
		echo "Missing dependencies: ${missing[*]}" >&2
		echo "Please install them and rerun setup." >&2
		exit 1
	fi
}

setup_rust_in_kernel() {
	local rust_version
	rust_version="$(scripts/min-tool-version.sh rustc)"
	
	echo "Setting up Rust $rust_version for kernel..."
	# 为内核目录设置 override
	rustup override set "$rust_version"	
	# 为项目根目录也设置相同的 override（用于 out-of-tree 模块）
	
	echo "Setting Rust override for project root..."
	popd >/dev/null
	rustup override set "$rust_version"
	rustup component add rust-src rustfmt clippy
	pushd "$KERNEL" >/dev/null
}

setup_kernel() {
	# setup the kernel
	ln -srf qemu-busybox-min.config $KERNEL/kernel/configs/qemu-busybox-min.config
	pushd "$KERNEL" >/dev/null
	setup_rust_in_kernel
	make LLVM=1 CLIPPY=1 rustavailable
	yes "" | make LLVM=1 CLIPPY=1 defconfig qemu-busybox-min.config rust.config || [ $? -eq 141 ]
	yes "" | make LLVM=1 CLIPPY=1 olddefconfig || [ $? -eq 141 ]
	make LLVM=1 CLIPPY=1 rust-analyzer
	popd >/dev/null
	# 为 out-of-tree 模块生成 rust-project.json
	make -C "$KERNEL" M="$CURRENT_DIR/src" LLVM=1 rust-analyzer
}

setup_busybox() {
	# setup busybox
	pushd "$BUSYBOX" >/dev/null
	yes "" | make defconfig || [ $? -eq 141 ]
	sed -i 's/.*CONFIG_STATIC.*/CONFIG_STATIC=y/' .config
	sed -i 's/.*CONFIG_STATIC_LIBGCC.*/CONFIG_STATIC_LIBGCC=y/' .config
	sed -i 's/.*CONFIG_TC.*/CONFIG_TC=n/' .config
	sed -i 's/.*CONFIG_FEATURE_TC_INGRESS.*/CONFIG_FEATURE_TC_INGRESS=n/' .config
	yes "" | make oldconfig || [ $? -eq 141 ]
	popd >/dev/null
}

sync_submodules() {
	echo "Synchronizing submodules (depth=${SUBMODULE_DEPTH})"
	git submodule sync --recursive
	git submodule update --init --recursive --depth "${SUBMODULE_DEPTH}" --recommend-shallow
}

KERNEL=./linux
BUSYBOX=./busybox
KERNEL_REPO=https://github.com/Rust-for-Linux/linux.git
BUSYBOX_REPO=https://git.busybox.net/busybox/
CURRENT_DIR=$PWD
SUBMODULE_DEPTH=${SUBMODULE_DEPTH:-1}

# ./scripts/setup -k /path/to/linux -b /path/to/busybox --busybox-repo repo --kernel-repo repo
while ((${#})); do
	case $1 in
	-k | --kernel)
		shift
		KERNEL=$1
		;;
	-b | --busybox)
		shift
		BUSYBOX=$1
		;;
	--kernel-repo)
		shift
		KERNEL_REPO=$1
		;;
	--busybox-repo)
		shift
		BUSYBOX_REPO=$1
		;;
	-h | --help)
		usage
		exit
		;;
	*)
		usage
		exit 1
		;;
	esac
	shift
done

# this script is a helper to setup a rust linux kernel development environment
# Please refer to the README.md for more information

echo "Checking dependencies"
check_dependencies

# install rustup only if needed
if ! instaled rustup; then
	echo "Installing rustup"
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
fi

sync_submodules

if [[ ! -d "$KERNEL" ]]; then
	echo "Kernel directory '$KERNEL' not found after syncing submodules" >&2
	exit 1
fi

if [[ ! -d "$BUSYBOX" ]]; then
	echo "BusyBox directory '$BUSYBOX' not found after syncing submodules" >&2
	exit 1
fi

echo "Setting up the kernel"
setup_kernel

echo "Setting up busybox"
setup_busybox

echo "Done! You can now build the kernel with: make build"
