# Copy eight signed integers, insertion-sort them, then sum them.
# At done: RAX = 42; output = [-19, -7, 0, 2, 3, 8, 13, 42].
.intel_syntax noprefix

.text
.global _start
.type _start, @function
_start:
    # Supply our own 16-byte-aligned stack before CALL.
    lea rsp, [rip + stack_top]
    lea rsi, [rip + input]
    lea rdi, [rip + output]
    # OFFSET makes the symbol an immediate operand, not a memory load.
    mov ecx, offset count
    cld
    rep movsq

    # System V AMD64 arguments: RDI = array, RSI = length.
    lea rdi, [rip + output]
    mov esi, offset count
    call insertion_sort

    xor eax, eax
    xor ecx, ecx
.Lsum:
    add rax, qword ptr [rdi + rcx * 8]
    inc rcx
    cmp rcx, rsi
    jb .Lsum
    mov qword ptr [rip + total], rax
done:
    # Oplab stops before this instruction; no operating system is required.
    jmp done
.size _start, . - _start

# Leaf function: insert each key into the already-sorted prefix.
# Uses only caller-saved registers; leaves RDI and RSI intact.
.type insertion_sort, @function
insertion_sort:
    mov eax, 1
.Lnext:
    cmp rax, rsi
    jae .Lreturn
    mov rdx, qword ptr [rdi + rax * 8]
    mov rcx, rax
.Lshift:
    test rcx, rcx
    jz .Linsert
    mov r8, qword ptr [rdi + rcx * 8 - 8]
    cmp r8, rdx
    jle .Linsert
    mov qword ptr [rdi + rcx * 8], r8
    dec rcx
    jmp .Lshift
.Linsert:
    mov qword ptr [rdi + rcx * 8], rdx
    inc rax
    jmp .Lnext
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

.section .note.GNU-stack,"",@progbits
