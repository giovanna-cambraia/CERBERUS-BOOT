#!/usr/bin/env bash
# Build the target application images (RV32IMAC, run in place from a slot):
#   v1_a      version 1, linked for slot A
#   v2_b      version 2, linked for slot B
#   v2bad_b   version 2 whose self-test always fails, linked for slot B
# Output: build/app_images/<name>.elf and <name>.bin
set -eu
source "$(dirname "${BASH_SOURCE[0]}")/rv32_env.sh"

FW="$HERE/firmware/app_c"
OUT="$HERE/build/app_images"
mkdir -p "$OUT/obj"

ARCH=(-march=rv32imac -mabi=ilp32)
CFLAGS=("${ARCH[@]}" -std=c11 -O2 -ffreestanding -fno-builtin
        -fno-tree-loop-distribute-patterns -ffunction-sections -fdata-sections
        -Wall -Wextra -Wpedantic -Wshadow -Wconversion -Wsign-conversion
        -Wcast-qual -Wstrict-prototypes -Werror
        -I"$FW/include")

build() {
  local name="$1" version="$2" origin="$3" selftest_fail="$4"
  local d="$OUT/obj/$name"
  rm -rf "$d"
  mkdir -p "$d"

  for f in "$FW/src/cerberus_nv.c" "$FW/src/ota_parser.c" "$FW/src/ota_agent.c" \
           "$FW/rt/app_platform.c" "$FW/rt/app_main.c" "$FW/rt/app_rt.c"; do
    "$CC" "${CFLAGS[@]}" -DAPP_VERSION="${version}u" -DAPP_SELFTEST_FAIL="$selftest_fail" \
          -c "$f" -o "$d/$(basename "${f%.c}").o"
  done
  "$CC" "${ARCH[@]}" -c "$FW/rt/app_start.S" -o "$d/app_start.o"

  sed "s/@ROM_ORIGIN@/$origin/" "$FW/rt/app.ld.in" > "$d/app.ld"
  "$CC" "${ARCH[@]}" -nostdlib -static -Wl,--gc-sections \
        -Wl,-T,"$d/app.ld" -Wl,-Map,"$OUT/$name.map" \
        -o "$OUT/$name.elf" "$d"/*.o
  "$OBJCOPY" -O binary "$OUT/$name.elf" "$OUT/$name.bin"
  echo "[app-images] $name: $(stat -c %s "$OUT/$name.bin") bytes, entry $("$NM" "$OUT/$name.elf" | awk '$3 == "_start" { print $1 }')"
}

build v1_a    1 0x40020100 0
build v2_b    2 0x40100100 0
build v2bad_b 2 0x40100100 1

echo "undefined symbols in v1_a (should be none):"
"$NM" -u "$OUT/v1_a.elf" || true