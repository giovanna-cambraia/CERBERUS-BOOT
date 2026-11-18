#!/usr/bin/env bash
# Derive a 32-bit runtime from the riscv64-elf GNAT's bare-metal runtime
# (Word_Size and Memory_Size 64 -> 32). Prints the directory on stdout.
set -eu
source "$(dirname "${BASH_SOURCE[0]}")/rv32_env.sh"

SRC="$(find_rts_src)" || { echo "no bare-metal runtime found under $TC" >&2; exit 1; }
OUT="$HERE/build/rts_rv32"
rm -rf "$OUT"
mkdir -p "$OUT"
cp "$SRC"/*.ads "$OUT"/
cp "$SRC"/*.adb "$OUT"/ 2>/dev/null || true
if [ -f "$SRC/gnat.adc" ]; then cp "$SRC/gnat.adc" "$OUT"/; fi

sed -E -i \
  -e 's/(Word_Size[[:space:]]*:[[:space:]]*constant[[:space:]]*:=[[:space:]]*)64/\132/' \
  -e 's/(Memory_Size[[:space:]]*:[[:space:]]*constant[[:space:]]*:=[[:space:]]*2[[:space:]]*\*\*[[:space:]]*)64/\132/' \
  "$OUT/system.ads"

if grep -nE '(Word_Size|Memory_Size)[[:space:]]*:[[:space:]]*constant[[:space:]]*:=.*64' \
     "$OUT/system.ads" >&2; then
  echo "system.ads still says 64 bits" >&2
  exit 1
fi
echo "runtime derived from $SRC" >&2
echo "$OUT"