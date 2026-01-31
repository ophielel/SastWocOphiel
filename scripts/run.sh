#!/bin/bash

set -euo pipefail

usage() {
	echo "Usage: $0 [-b /path/to/busybox] [-k /path/to/kernel]"
}

requires() {
	for i in "$@"; do
		if ! command -v "$i" &>/dev/null; then
			echo "Error: $i is required but not installed."
			exit 1
		fi
	done
}

# Dependencies
requires qemu-system-x86_64 cpio gzip

BUSYBOX=./busybox
KERNEL=./linux

# ./scripts/run -b /path/to/busybox -k /path/to/kernel
while getopts "b:k:" opt; do
	case $opt in
	b)
		BUSYBOX=$OPTARG
		;;
	k)
		KERNEL=$OPTARG
		;;
	\?)
		echo "Invalid option: -$OPTARG" >&2
		exit 1
		;;
	:)
		echo "Option -$OPTARG requires an argument." >&2
		exit 1
		;;
	esac
done

# run the kernel in qemu
qemu-system-x86_64 \
	-kernel "$KERNEL"/arch/x86_64/boot/bzImage \
	-initrd "$BUSYBOX"/rootfs.img \
    -nographic \
    -machine q35 \
    -device intel-iommu \
    -cpu qemu64 \
    -m 4G \
    -nic user,model=virtio-net-pci,hostfwd=tcp::5555-:23,hostfwd=tcp::5556-:8080 \
    -append "console=ttyS0,115200 loglevel=3 rdinit=/sbin/init"
