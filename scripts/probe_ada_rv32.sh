#!/usr/bin/env bash
# Probe: can a GNAT riscv64-elf cross toolchain build 32-bit (rv32imac/ilp32)
# Ada for BL1? Prints a PASS/FAIL line per link of the chain.
#
#   scripts/probe_ada_rv32.sh [toolchain dir]
#
# The toolchain dir is found in ~/tools/gnat_riscv64_elf_* if not given.
# Everything is written under build/ada_rv32_probe/.
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TC="${1:-${GNAT_RV:-}}"
if [ -z "$TC" ]; then
  TC="$(ls -d "$HOME"/tools/gnat_riscv64_elf_* 2>/dev/null | tail -n1)"
fi
if [ -z "$TC" ] || [ ! -d "$TC/bin" ]; then
  echo "No riscv64-elf GNAT toolchain found. Install it with:"
  echo "  cd ~/tools && alr get gnat_riscv64_elf"
  echo "or pass its directory as the first argument."
  exit 2
fi

BIN="$TC/bin"
CC="$BIN/riscv64-elf-gcc"
READELF="$BIN/riscv64-elf-readelf"
NM="$BIN/riscv64-elf-nm"
SIZE="$BIN/riscv64-elf-size"
W="$HERE/build/ada_rv32_probe"
rm -rf "$W"
mkdir -p "$W"

pass()    { echo "[PASS] $*"; }
fail()    { echo "[FAIL] $*"; }
note()    { echo "       $*"; }
section() { echo; echo "== $*"; }
indent()  { sed 's/^/       /'; }

elf_class() {
  "$READELF" -h "$1" 2>/dev/null | awk -F: '/Class/ { gsub(/ /, "", $2); print $2 }'
}

# ---------------------------------------------------------------------------
section "1. Toolchain"
echo "directory: $TC"
echo "tools:     $(ls "$BIN" | grep -E 'gcc$|gnat|gprbuild|-ld$|-as$' | tr '\n' ' ')"
if [ ! -x "$CC" ]; then
  fail "$CC not found (tool names differ?)"
  exit 1
fi
"$CC" --version 2>&1 | head -1
"$CC" -v 2>&1 | grep -E '^Target:' | indent
echo "configured with:"
"$CC" -v 2>&1 | grep 'Configured with:' | sed 's/Configured with: //' | fold -w 90 -s | indent
echo "multilibs:"
"$CC" -print-multi-lib 2>&1 | indent

# ---------------------------------------------------------------------------
section "2. 32-bit C code from the riscv64 compiler"
cat > "$W/t.c" <<'EOF'
unsigned add(unsigned a, unsigned b) { return a + b; }
unsigned long long mul64(unsigned long long a, unsigned long long b) { return a * b; }
EOF
cat > "$W/start.S" <<'EOF'
        .section .text.init, "ax"
        .globl _start
_start:
        j _start
EOF
if "$CC" -c -march=rv32imac -mabi=ilp32 -O1 -o "$W/t.o" "$W/t.c" 2> "$W/t.err"; then
  cls="$(elf_class "$W/t.o")"
  if [ "$cls" = "ELF32" ]; then
    pass "compiling C for rv32imac/ilp32 gives an $cls object"
  else
    fail "compiling worked but the object is $cls, not ELF32"
  fi
else
  fail "compiling C for rv32imac/ilp32:"
  indent < "$W/t.err"
fi

if "$CC" -march=rv32imac -mabi=ilp32 -nostdlib -static -Wl,-Ttext=0x40000100 \
     -o "$W/t.elf" "$W/start.S" "$W/t.o" 2> "$W/link.err"; then
  pass "linking a 32-bit ELF ($(elf_class "$W/t.elf"))"
else
  fail "linking a 32-bit ELF:"
  indent < "$W/link.err"
fi

# ---------------------------------------------------------------------------
section "3. Ada runtimes shipped with the toolchain"
find "$TC" -name system.ads 2>/dev/null | while read -r f; do
  d="$(dirname "$f")"
  echo "$d"
  grep -E '^[[:space:]]*(Word_Size|Memory_Size)[[:space:]]*:' "$f" | indent
  for u in ada.ads interfac.ads gnat.ads; do
    if [ -f "$d/$u" ]; then note "has $u"; else note "missing $u"; fi
  done
done

RTS_SRC=""
for pat in zfp light; do
  cand="$(find "$TC" -name system.ads -path "*$pat*" 2>/dev/null | head -n1)"
  if [ -n "$cand" ]; then RTS_SRC="$(dirname "$cand")"; break; fi
done
if [ -z "$RTS_SRC" ]; then
  cand="$(find "$TC" -name system.ads 2>/dev/null | head -n1)"
  if [ -n "$cand" ]; then RTS_SRC="$(dirname "$cand")"; fi
fi
if [ -z "$RTS_SRC" ]; then
  fail "no system.ads anywhere in the toolchain: there is no runtime to start from"
  echo
  echo "VERDICT: C works for rv32 but there is no Ada runtime to adapt. See 'If this fails' below."
  exit 1
fi
pass "starting point for a 32-bit runtime: $RTS_SRC"

# ---------------------------------------------------------------------------
section "4. Derive a 32-bit runtime (Word_Size and Memory_Size 64 -> 32)"
RTS="$W/rts"
mkdir -p "$RTS"
cp "$RTS_SRC"/*.ads "$RTS"/ 2>/dev/null
cp "$RTS_SRC"/*.adb "$RTS"/ 2>/dev/null
sed -E -i \
  -e 's/(Word_Size[[:space:]]*:[[:space:]]*constant[[:space:]]*:=[[:space:]]*)64/\132/' \
  -e 's/(Memory_Size[[:space:]]*:[[:space:]]*constant[[:space:]]*:=[[:space:]]*2[[:space:]]*\*\*[[:space:]]*)64/\132/' \
  "$RTS/system.ads"
echo "changes to system.ads:"
diff -u "$RTS_SRC/system.ads" "$RTS/system.ads" | grep -E '^[+-][^+-]' | indent
echo "remaining 64s in system.ads:"
grep -nE '64' "$RTS/system.ads" | indent

# ---------------------------------------------------------------------------
section "5. Compile the BL1 SPARK units for rv32imac/ilp32"
SRC="$HERE/firmware/bl1_spark/src"
OBJ="$W/obj"
mkdir -p "$OBJ"
FLAGS=(-c -march=rv32imac -mabi=ilp32 -O1 -gnat2022 -gnatp -nostdinc
       -I"$RTS" -I"$SRC" -ffunction-sections -fdata-sections)
all_ok=1
for f in bl1.ads bl1-types.ads bl1-crc.adb bl1-header.adb bl1-policy.adb; do
  if (cd "$OBJ" && "$CC" "${FLAGS[@]}" "$SRC/$f") 2> "$OBJ/$f.err"; then
    pass "compiled $f"
  else
    fail "compiling $f"
    head -20 "$OBJ/$f.err" | indent
    all_ok=0
  fi
done

# ---------------------------------------------------------------------------
section "6. What the objects need and weigh"
if [ "$all_ok" -eq 1 ]; then
  for o in "$OBJ"/*.o; do
    [ -f "$o" ] || continue
    echo "$(basename "$o"): $(elf_class "$o")"
  done
  echo "undefined symbols (what a runtime or start-up code must provide):"
  "$NM" -u "$OBJ"/*.o 2>/dev/null | sort -u | indent
  echo "sizes:"
  "$SIZE" "$OBJ"/*.o 2>/dev/null | indent
fi

# ---------------------------------------------------------------------------
echo
echo "== Verdict"
if [ "$all_ok" -eq 1 ]; then
  echo "The riscv64 GNAT compiled all BL1 units to 32-bit code with a derived runtime."
  echo "Paste this whole output so I can check the object class and the undefined symbols."
else
  echo "Compilation failed. Paste this whole output; the first error above is the one that matters."
fi