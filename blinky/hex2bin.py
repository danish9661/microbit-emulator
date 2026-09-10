#!/usr/bin/env python3
"""Intel HEX -> nRF52833 flash image splitter.
Keeps [0x0, 0x80000) as a 512KB .bin (0xFF fill) and prints any records
outside flash (UICR @0x10001000 etc.) so tests can seed them explicitly.
Usage: hex2bin.py in.hex out.bin
"""
import sys

FLASH_END = 0x80000

def main():
    src, dst = sys.argv[1], sys.argv[2]
    flash = bytearray([0xFF]) * FLASH_END
    base = 0
    extra = []
    for line in open(src):
        line = line.strip()
        if not line.startswith(':'):
            continue
        n = int(line[1:3], 16)
        addr = int(line[3:7], 16)
        typ = int(line[7:9], 16)
        if typ == 2:
            base = int(line[9:13], 16) << 4
        elif typ == 4:
            base = int(line[9:13], 16) << 16
        elif typ == 0:
            at = base + addr
            data = bytes(int(line[9 + i * 2:11 + i * 2], 16) for i in range(n))
            if at < FLASH_END:
                flash[at:at + n] = data
            else:
                extra.append((at, data))
    open(dst, 'wb').write(flash)
    print(f'flash: {dst} ({len(flash)} bytes)')
    for base, data in extra:
        print(f'extra @{base:#010x} ({len(data)}B): {data.hex()}')

if __name__ == '__main__':
    main()
