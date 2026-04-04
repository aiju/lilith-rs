#!/bin/sh
qemu-system-x86_64 -serial stdio -kernel "$1" -device isa-debug-exit,iobase=0xf4,iosize=0x04 $QEMU_EXTRA
status=$?
if [ $status -eq 33 ]; then
    exit 0
else
    exit 1
fi
