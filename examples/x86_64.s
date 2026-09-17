# Brighten eight straight-alpha RGBA8 pixels with AVX2.
# Add 32 to RGB, clamp at 255, preserve alpha. At done: RAX = checksum = 4814.
.intel_syntax noprefix

.text
.global _start
.type _start, @function
_start:
    vmovdqu ymm0, [rip + input]
    vpaddusb ymm0, ymm0, [rip + bias]
    vmovdqu [rip + output], ymm0
    vpxor ymm1, ymm1, ymm1
    vpsadbw ymm0, ymm0, ymm1
    vextracti128 xmm1, ymm0, 1
    vpaddq xmm0, xmm0, xmm1
    vpsrldq xmm1, xmm0, 8
    vpaddq xmm0, xmm0, xmm1
    vmovq rax, xmm0
    mov [rip + checksum], rax
done:
    # Completion boundary; no operating system or stack is needed.
    jmp done
.size _start, . - _start

.section .rodata
.p2align 5
bias:
    .rept 8
    .byte 32, 32, 32, 0
    .endr
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
.p2align 5
output:
    .skip bytes
checksum:
    .skip 8

.section .note.GNU-stack,"",@progbits
