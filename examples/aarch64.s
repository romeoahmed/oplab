// Copy eight signed integers, insertion-sort them, then sum them.
// At done: X0 = 42; output = [-19, -7, 0, 2, 3, 8, 13, 42].
.text
.p2align 2
.global _start
.type _start, %function
_start:
    // Page-relative relocations work across text and data pages.
    adrp x9, stack_top
    add sp, x9, :lo12:stack_top
    adrp x2, input
    add x2, x2, :lo12:input
    adrp x3, output
    add x3, x3, :lo12:output
    mov x4, #count
.Lcopy:
    ldr x5, [x2], #8
    str x5, [x3], #8
    subs x4, x4, #1
    b.ne .Lcopy

    // AAPCS64 arguments: X0 = array, X1 = length.
    adrp x0, output
    add x0, x0, :lo12:output
    mov x1, #count
    bl insertion_sort

    mov x2, x0
    mov x0, #0
.Lsum:
    ldr x3, [x2], #8
    add x0, x0, x3
    subs x1, x1, #1
    b.ne .Lsum
    adrp x2, total
    str x0, [x2, :lo12:total]
done:
    // Oplab stops before this instruction; no operating system is required.
    b done
.size _start, . - _start

// Leaf function: insert each key into the already-sorted prefix.
// Uses only caller-saved registers; leaves X0 and X1 intact.
.type insertion_sort, %function
insertion_sort:
    mov x2, #1
.Lnext:
    cmp x2, x1
    b.hs .Lreturn
    ldr x3, [x0, x2, lsl #3]
    mov x4, x2
.Lshift:
    cbz x4, .Linsert
    sub x5, x4, #1
    ldr x6, [x0, x5, lsl #3]
    cmp x6, x3
    b.le .Linsert
    str x6, [x0, x4, lsl #3]
    mov x4, x5
    b .Lshift
.Linsert:
    str x3, [x0, x4, lsl #3]
    add x2, x2, #1
    b .Lnext
.Lreturn:
    ret
.size insertion_sort, . - insertion_sort

.section .rodata
.p2align 3
input:
    .quad 13, -7, 42, 0, -19, 8, 3, 2
.equ count, (. - input) / 8

.bss
.p2align 3
output:
    .skip count * 8
total:
    .skip 8
.p2align 4
stack:
    .skip 4096
stack_top:

.section .note.GNU-stack,"",%progbits
