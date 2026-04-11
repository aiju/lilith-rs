#!/bin/sh
kernel="$1"
shift 1
qemu-system-x86_64 -serial stdio -kernel "$kernel" -initrd ../fs.img -device isa-debug-exit,iobase=0xf4,iosize=0x04 -append "$*" $QEMU_EXTRA
status=$?
if [ $status -eq 33 ]; then
    exit 0
else
    exit 1
fi
