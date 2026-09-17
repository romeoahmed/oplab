// Brighten eight straight-alpha RGBA8 pixels with vector-length-agnostic SVE2.
// Add 32 to RGB, clamp at 255, preserve alpha. At done: X0 = checksum = 4814.
.arch armv9-a
.text
.p2align 2
.global _start
.type _start, %function
_start:
    adrp x1, input
    add x1, x1, :lo12:input
    adrp x2, output
    add x2, x2, :lo12:output
    mov z1.b, #32
    and z1.s, z1.s, #0x00ffffff  // Preserve alpha: repeat the byte bias 32, 32, 32, 0.
    mov x0, #0
    mov x4, #0
    mov x5, #bytes
    whilelo p0.b, x4, x5
.Lpixels:
    ld1b z0.b, p0/z, [x1, x4]
    uqadd z0.b, p0/m, z0.b, z1.b
    st1b z0.b, p0, [x2, x4]
    uaddv d2, p0, z0.b
    fmov x3, d2
    add x0, x0, x3
    incb x4
    whilelo p0.b, x4, x5
    b.any .Lpixels

    adrp x1, checksum
    str x0, [x1, :lo12:checksum]
done:
    // Completion boundary; no operating system or stack is needed.
    b done
.size _start, . - _start

.section .rodata
.p2align 4
input:
    .byte   0,  32,  64, 255
    .byte 128, 192, 223, 128
    .byte 224, 240, 255,  64
    .byte 255,   0,  16,   0
    .byte  12,  48,  96, 192
    .byte 200, 220, 222, 254
    .byte   1, 127, 254,   1
    .byte  16,  80, 160, 200
.equ bytes, . - input

.bss
.p2align 4
output:
    .skip bytes
.p2align 3
checksum:
    .skip 8

.section .note.GNU-stack,"",%progbits
