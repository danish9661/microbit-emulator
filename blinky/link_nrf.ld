MEMORY { FLASH (rx) : ORIGIN = 0x00000000, LENGTH = 512K  RAM (rwx) : ORIGIN = 0x20000000, LENGTH = 128K }
SECTIONS {
  .vectors : { KEEP(*(.vectors)) } > FLASH
  .text : { *(.text*) } > FLASH
  .rodata : { *(.rodata*) } > FLASH
  _srodata = .;
}
