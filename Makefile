# SPDX-License-Identifier: GPL-2.0

KDIR ?= ./linux
BDIR ?= ./busybox
TDIR ?= ./tools
SUBMODULE_DEPTH ?= 1
TARGET ?=
NCPU ?= $(shell nproc)

BZIMAGE := $(KDIR)/arch/x86_64/boot/bzImage
ROOTFS := $(BDIR)/rootfs.img
BUSYBOX_BIN := $(BDIR)/busybox
BUSYBOX_INSTALL := $(BDIR)/_install

USERSPACE_PROG := play_tetris
MAGIC_TEST_PROG := test_magic

ROOTFS_STAMP := .rootfs_stamp

.PHONY: all run build setup clean rebuild kernel busybox rootfs module module-clean module-install tools tools-clean tools-install repack-rootfs

all: run

build: kernel busybox $(ROOTFS) module-install tools-install repack-rootfs

rebuild: clean-build build

kernel: $(BZIMAGE)

$(BZIMAGE):
	@echo "Building linux kernel..."
	@cd $(KDIR) && yes "" | make LLVM=1 CLIPPY=1 $(TARGET) -j$(NCPU) || [ $$? -eq 141 ]

busybox: $(BUSYBOX_BIN)

$(BUSYBOX_BIN):
	@echo "Building busybox..."
	@cd $(BDIR) && yes "" | make -j$(NCPU) || [ $$? -eq 141 ]

rootfs: $(ROOTFS)

$(ROOTFS): $(BUSYBOX_BIN) | $(BUSYBOX_INSTALL)
	@echo "Installing busybox..."
	@cd $(BDIR) && make install
	@echo "Configuring rootfs..."
	@scripts/config-rootfs.sh -b $(BDIR)
	@$(MAKE) repack-rootfs

$(BUSYBOX_INSTALL):
	@mkdir -p $(BUSYBOX_INSTALL)

MODULE_NAME := woc2026_hello_from_skm
MODULE_SRC := src
MODULE_KO := $(MODULE_SRC)/$(MODULE_NAME).ko

module: kernel
	@echo "Preparing Rust environment..."
	@$(MAKE) -C $(KDIR) LLVM=1 CLIPPY=1 prepare
	@echo "Building Rust kernel module..."
	$(MAKE) -C $(KDIR) M=$(PWD)/$(MODULE_SRC) LLVM=1 modules

module-clean:
	@echo "Cleaning Rust module..."
	$(MAKE) -C $(KDIR) M=$(PWD)/$(MODULE_SRC) clean

module-install: module
	@echo "Installing module to rootfs..."
	@mkdir -p $(BUSYBOX_INSTALL)/lib/modules
	@cp $(MODULE_KO) $(BUSYBOX_INSTALL)/lib/modules/
	@cp $(MODULE_SRC)/magic.ko $(BUSYBOX_INSTALL)/lib/modules/
	@touch $(ROOTFS_STAMP)

tools: $(TDIR)/$(USERSPACE_PROG).c
	@echo "Building userspace program..."
	@gcc -static -o $(TDIR)/$(USERSPACE_PROG).a $(TDIR)/$(USERSPACE_PROG).c -Wall -Wextra

tools-clean:
	@echo "Cleaning userspace program..."
	@rm -f $(TDIR)/$(USERSPACE_PROG).a

tools-install: tools
	@echo "Installing userspace tools..."
	@cp $(TDIR)/$(USERSPACE_PROG).a $(BUSYBOX_INSTALL)/bin/$(USERSPACE_PROG)
	@cp $(TDIR)/$(USERSPACE_PROG).a $(BUSYBOX_INSTALL)/usr/bin/$(USERSPACE_PROG)
	@touch $(ROOTFS_STAMP)

repack-rootfs: $(BUSYBOX_INSTALL)
	@echo "Packing rootfs image..."; \
	cd $(BUSYBOX_INSTALL) && find . | cpio -o -H newc | gzip > ../rootfs.img; \
	touch $(ROOTFS_STAMP);

run: build
	@echo "Starting QEMU..."
	scripts/run.sh -b $(BDIR) -k $(KDIR)

setup:
	SUBMODULE_DEPTH=$(SUBMODULE_DEPTH) scripts/setup.sh

clean-build:
	@echo "Cleaning build artifacts..."
	@rm -f $(BZIMAGE) $(ROOTFS) $(ROOTFS_STAMP)
	@rm -rf $(BUSYBOX_INSTALL)

clean: clean-build module-clean tools-clean
	@echo "Cleaning kernel..."
	@$(MAKE) -C $(KDIR) clean 2>/dev/null || true
	@echo "Cleaning busybox..."
	@$(MAKE) -C $(BDIR) clean 2>/dev/null || true
