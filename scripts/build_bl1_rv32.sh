#!/usr/bin/env bash
# Build BL1 (Ada/SPARK core, Monocypher, C start-up) for RV32IMAC with the
# riscv64-elf GNAT. Output: build/bl1_rv32/bl1.elf, bl1.bin, bl1.map
set -eu
source "$(dirname "${BASH_SOURCE[0]}")/rv32_env.sh"

RTS="$("$HERE/scripts/make_rv32_rts.sh")"
FW="$HERE/firmware/bl1_spark"
THIRD="$FW/third_party/monocypher"
OUT="$HERE/build/bl1_rv32"
rm -rf "$OUT"
mkdir -p "$OUT/obj"

ARCH=(-march=rv32imac -mabi=ilp32)
SECTIONS=(-ffunction-sections -fdata-sections -fno-tree-loop-distribute-patterns)

ADA_FLAGS=("${ARCH[@]}" -c -O1 -gnat2022 -gnatp -gnatwa -gnatwe -nostdinc
           -I"$RTS" -I"$FW/src" -gnatec="$FW/gnat.adc" "${SECTIONS[@]}")
if [ -f "$RTS/gnat.adc" ]; then
  ADA_FLAGS+=(-gnatec="$RTS/gnat.adc")
fi
C_FLAGS=("${ARCH[@]}" -c -O1 -ffreestanding -fno-builtin -Wall -Wextra -Werror "${SECTIONS[@]}")
C3P_FLAGS=("${ARCH[@]}" -c -Os -std=c99 -ffreestanding -fno-builtin -Wall -Wextra
           "${SECTIONS[@]}" -I"$THIRD")

echo "[bl1-rv32] compiling Ada"
for f in bl1.ads bl1-types.ads bl1-crc.adb bl1-header.adb bl1-policy.adb \
         bl1-sha256.adb bl1-ed25519.adb bl1-platform.adb bl1-boot.adb bl1_main.adb; do
  (cd "$OUT/obj" && "$CC" "${ADA_FLAGS[@]}" "$FW/src/$f")
done

echo "[bl1-rv32] compiling C (start-up, runtime, Monocypher, glue)"
"$CC" "${ARCH[@]}" -c -o "$OUT/obj/start.o" "$FW/rt/start.S"
"$CC" "${C_FLAGS[@]}" -o "$OUT/obj/rt.o" "$FW/rt/rt.c"
for f in monocypher monocypher-ed25519; do
  "$CC" "${C3P_FLAGS[@]}" -o "$OUT/obj/$f.o" "$THIRD/$f.c"
done
"$CC" "${C3P_FLAGS[@]}" -Werror -o "$OUT/obj/crypto_glue.o" "$FW/glue/crypto_glue.c"

# Compiler helper routines (64-bit shifts and so on), if this toolchain has a
# libgcc for rv32imac/ilp32.
LIBGCC="$("$CC" "${ARCH[@]}" -print-libgcc-file-name)"
LIBS=()
if [ -f "$LIBGCC" ]; then
  LIBS=("$LIBGCC")
  echo "[bl1-rv32] libgcc: $LIBGCC"
else
  echo "[bl1-rv32] no libgcc for rv32imac/ilp32; helpers must come from rt.c"
fi

echo "[bl1-rv32] linking"
"$CC" "${ARCH[@]}" -nostdlib -static -Wl,--gc-sections \
      -Wl,-T,"$FW/rt/bl1.ld" -Wl,-Map,"$OUT/bl1.map" \
      -o "$OUT/bl1.elf" \
      "$OUT/obj/start.o" "$OUT"/obj/bl1*.o \
      "$OUT/obj/crypto_glue.o" "$OUT/obj/monocypher.o" "$OUT/obj/monocypher-ed25519.o" \
      "$OUT/obj/rt.o" "${LIBS[@]}"
"$OBJCOPY" -O binary "$OUT/bl1.elf" "$OUT/bl1.bin"

"$SIZE" "$OUT/bl1.elf"
bytes="$(stat -c %s "$OUT/bl1.bin")"
echo "bl1.bin: $bytes bytes (boot ROM is 32768)"
if [ "$bytes" -gt 32768 ]; then
  echo "BL1 does not fit the boot ROM" >&2
  exit 1
fi
echo "undefined symbols (should be none):"
"$NM" -u "$OUT/bl1.elf" || true

echo "[bl1-rv32] SHA-256 object (not in the ROM until BL1 calls it):"
"$SIZE" "$OUT/obj/bl1-sha256.o"

echo "[bl1-rv32] size probe: ROM cost of the Ed25519 check once BL1 calls it"
"$CC" "${C_FLAGS[@]}" -o "$OUT/obj/size_probe.o" "$FW/rt/size_probe.c"
if "$CC" "${ARCH[@]}" -nostdlib -static -Wl,--gc-sections -Wl,-e,probe_entry \
     -Wl,-T,"$FW/rt/bl1.ld" -o "$OUT/probe.elf" \
     "$OUT/obj/size_probe.o" "$OUT/obj/crypto_glue.o" \
     "$OUT/obj/monocypher.o" "$OUT/obj/monocypher-ed25519.o" \
     "$OUT/obj/rt.o" "${LIBS[@]}"; then
  "$SIZE" "$OUT/probe.elf"
else
  echo "size probe did not link: the undefined symbols above are what is missing"
fi