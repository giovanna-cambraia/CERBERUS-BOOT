# Sourced by the RV32 Ada build scripts: finds the riscv64-elf GNAT toolchain
# and sets HERE, TC, BIN, CC, OBJCOPY, NM, SIZE. Not meant to be run directly.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TC="${GNAT_RV:-}"
if [ -z "$TC" ]; then
  TC="$(ls -d "$HOME"/tools/gnat_riscv64_elf_* 2>/dev/null | tail -n1)"
fi
if [ -z "$TC" ] || [ ! -x "$TC/bin/riscv64-elf-gcc" ]; then
  echo "riscv64-elf GNAT toolchain not found." >&2
  echo "Install it with 'cd ~/tools && alr get gnat_riscv64_elf' or set GNAT_RV." >&2
  return 1 2>/dev/null || exit 1
fi
BIN="$TC/bin"
CC="$BIN/riscv64-elf-gcc"
OBJCOPY="$BIN/riscv64-elf-objcopy"
NM="$BIN/riscv64-elf-nm"
SIZE="$BIN/riscv64-elf-size"

# Directory of a bare-metal runtime to adapt (has system.ads, ada.ads, interfac.ads).
find_rts_src() {
  local pat cand d
  for pat in zfp light ""; do
    while read -r cand; do
      d="$(dirname "$cand")"
      if [ -f "$d/interfac.ads" ] && [ -f "$d/ada.ads" ]; then
        echo "$d"
        return 0
      fi
    done < <(find "$TC" -name system.ads -path "*${pat}*" 2>/dev/null)
  done
  return 1
}