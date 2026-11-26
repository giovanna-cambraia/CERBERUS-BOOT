#!/usr/bin/env bash
# Compile the OTA agent for RV32IMAC (freestanding, strict warnings). Objects only:
# linking into a target image is the next step.
set -eu
source "$(dirname "${BASH_SOURCE[0]}")/rv32_env.sh"

FW="$HERE/firmware/app_c"
OUT="$HERE/build/app_rv32"
mkdir -p "$OUT"

FLAGS=(-march=rv32imac -mabi=ilp32 -std=c11 -O2 -ffreestanding -fno-builtin
       -ffunction-sections -fdata-sections
       -Wall -Wextra -Wpedantic -Wshadow -Wconversion -Wsign-conversion
       -Wcast-qual -Wstrict-prototypes -Werror
       -I"$FW/include")

for f in cerberus_nv ota_parser ota_agent; do
  "$CC" "${FLAGS[@]}" -c "$FW/src/$f.c" -o "$OUT/$f.o"
done
echo "[app-rv32] objects in $OUT:"
"$SIZE" "$OUT"/*.o