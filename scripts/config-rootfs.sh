#!/bin/bash

set -euo pipefail

usage() {
	echo "Usage: $0 -b /path/to/busybox" 1>&2
	echo "Configure rootfs for busybox installation" 1>&2
	exit 1
}

config_rootfs() {
	local busybox_dir="$1"
	pushd "$busybox_dir/_install" >/dev/null
	
	mkdir -p usr/share/udhcpc/ etc/init.d/ sbin
	
	# Create init symlink if it doesn't exist
	if [ ! -e sbin/init ]; then
		ln -s ../bin/busybox sbin/init
	fi
	
	cat <<'EOF' >etc/init.d/rcS
#!/bin/sh
mkdir -p /proc /sys
mount -t proc none /proc
mount -t sysfs none /sys
mount -t devtmpfs none /dev
mkdir -p /dev/pts
mount -t devpts nodev /dev/pts
ifconfig lo up
ifconfig eth0 up
udhcpc -i eth0
telnetd -l /bin/sh
insmod /lib/modules/magic.ko

clear
cat <<'BANNER'
                                   ...,c,,      
Welcome to                     . ., 0.l ':       
                            c c' ' c'   l       
        ,odc      .ck    .,.;' .  .';loodoo,.   
     .X..'cXX    .XWM,  ';'..,cxxdl;..   .k0:   
     'WK.   .   ;0,oMx   ..dkl'...c' .:;;KMo'.  
      'NN,     ,o  'NW  :0d. :0N:kN.,:;ddX,'    
       .OKc  .kl'cl;KMd0O    xMMk.    .lN.      
   ..   .Ok ,NXk;   oMMK       ;kWl   'NN0:     
  .;lkxdc.  c;      .ck     ...',c.   0O'       
        
Try:
  - Read kernel logs:  dmesg | tail
  - List modules:      lsmod
  - Explore devices:   ls -l /dev
  - Execute the tool:  play_tetris
BANNER
EOF
	
	chmod a+x etc/init.d/rcS bin/* sbin/*
	
	cat <<'EOF' >etc/inittab
::sysinit:/etc/init.d/rcS
::respawn:-/bin/sh
::ctrlaltdel:/sbin/reboot
::ctrlaltbreak:/sbin/poweroff
#::shutdown:/bin/umount -a -r
#::shutdown:/sbin/swapoff -a
EOF
	
	cp ../examples/udhcp/simple.script usr/share/udhcpc/default.script
	
	popd >/dev/null
}

BUSYBOX=./busybox

while getopts b:h option; do
	case "$option" in
	b) BUSYBOX=${OPTARG} ;;
	h) usage ;;
	*) usage ;;
	esac
done

if [[ ! -d "$BUSYBOX" ]]; then
	echo "Busybox directory '$BUSYBOX' not found" >&2
	exit 1
fi

if [[ ! -d "$BUSYBOX/_install" ]]; then
	echo "Busybox not installed yet, please run 'make busybox' first" >&2
	exit 1
fi

config_rootfs "$BUSYBOX"
