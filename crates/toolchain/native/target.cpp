#include "target.hpp"
#include <llvm/Support/TargetSelect.h>
#include <mutex>
namespace oplab {
void initialize_targets() {
  static std::once_flag once;
  std::call_once(once, [] {
    LLVMInitializeX86TargetInfo();
    LLVMInitializeX86TargetMC();
    LLVMInitializeX86AsmParser();
    LLVMInitializeAArch64TargetInfo();
    LLVMInitializeAArch64TargetMC();
    LLVMInitializeAArch64AsmParser();
    LLVMInitializeAArch64Disassembler();
  });
}
} // namespace oplab
