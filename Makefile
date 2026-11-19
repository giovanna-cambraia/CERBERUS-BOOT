# =============================================================================
# CERBERUS-BOOT top-level Makefile (native Linux x86_64 / WSL2, no Docker)
# =============================================================================

SHELL := /usr/bin/env bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help

# ---- Layout -----------------------------------------------------------------
BUILD      := build
GHDL_WORK  := $(BUILD)/ghdl
HDL_SRC    := $(wildcard hdl/src/*.vhd)
HDL_TB     := $(wildcard hdl/tb/*.vhd)
HDL_TOP    := rot_peripheral_tb

BL1_DIR    := firmware/bl1_spark
BL1_GPR    := $(BL1_DIR)/bl1.gpr
BL2_DIR    := firmware/bl2_rust
APP_DIR    := firmware/app_c
CLI_DIR    := tools/cerberus-cli
BRIDGE_DIR := sim/bridge
RESC       := scripts/platform_co_sim.resc

# ---- Toolchains (override on the command line, e.g. make CC=clang) ----------
GHDL       ?= ghdl
GHDL_STD   ?= 08
GNATPROVE  ?= gnatprove
GPRBUILD   ?= gprbuild
CARGO      ?= cargo
RV_TARGET  ?= riscv32imac-unknown-none-elf
RV_CC      ?= riscv64-unknown-elf-gcc
HOST_CC    ?= clang
RENODE     ?= renode
BRIDGE_PORT?= 9999

APP_SRC    := $(wildcard $(APP_DIR)/src/*.c)
APP_TEST   := $(wildcard $(APP_DIR)/test/*.c)

.PHONY: all help check-tools dirs \
        hdl bl1 bl2 app bridge cli \
        test test-hdl prove test-bl2 kani fuzz-app \
        cosim cosim-bridge sign-keys clean distclean

# ---- Help -------------------------------------------------------------------
help: ## show this help
	@echo "CERBERUS-BOOT targets:"
	@grep -hE '^[a-zA-Z0-9_-]+:.*## ' $(MAKEFILE_LIST) \
	  | awk -F':.*## ' '{ printf "  %-14s %s\n", $$1, $$2 }'

# ---- Tool check -------------------------------------------------------------
check-tools: ## report which required tools are installed
	@for t in $(GHDL) $(GNATPROVE) $(GPRBUILD) $(CARGO) $(RV_CC) $(HOST_CC) $(RENODE); do \
	  if command -v $$t >/dev/null 2>&1; then printf "  [ok]      %s\n" $$t; \
	  else printf "  [missing] %s\n" $$t; fi; \
	done
	@if $(CARGO) kani --version >/dev/null 2>&1; then echo "  [ok]      cargo-kani"; \
	  else echo "  [missing] cargo-kani"; fi

dirs:
	@mkdir -p $(BUILD) $(GHDL_WORK)

# ---- Build ------------------------------------------------------------------
all: hdl bl1 bl2 app cli bridge ## build every component that exists

hdl: dirs ## analyse + elaborate the VHDL RoT (GHDL)
	@if [ -z "$(HDL_SRC)" ]; then echo "[skip] hdl: no .vhd files in hdl/src yet"; else \
	  echo "[hdl] analysing"; \
	  $(GHDL) -a --std=$(GHDL_STD) --workdir=$(GHDL_WORK) $(HDL_SRC) $(HDL_TB); \
	fi

bl1: dirs ## build Stage-1 bootloader (Ada/SPARK)
	@if [ -f $(BL1_GPR) ]; then echo "[bl1] gprbuild"; $(GPRBUILD) -P $(BL1_GPR) -p; \
	else echo "[skip] bl1: $(BL1_GPR) not present yet"; fi

bl2: dirs ## build Stage-2 bootloader (Rust no_std, RV32IMAC)
	@if [ -f $(BL2_DIR)/Cargo.toml ]; then echo "[bl2] cargo build"; \
	  $(CARGO) build --release --target $(RV_TARGET) --features crypto --manifest-path $(BL2_DIR)/Cargo.toml; \
	else echo "[skip] bl2: $(BL2_DIR)/Cargo.toml not present yet"; fi

app: dirs ## build application + OTA agent (C11, cross-compiled)
	@if [ -z "$(APP_SRC)" ]; then echo "[skip] app: no .c files in $(APP_DIR)/src yet"; else \
	  echo "[app] cross-compiling"; \
	  $(RV_CC) -std=c11 -march=rv32imac -mabi=ilp32 -Wall -Wextra -Werror \
	    -ffreestanding -I$(APP_DIR)/include -c $(APP_SRC) \
	    --output-dir=$(BUILD) 2>/dev/null || \
	  (cd $(BUILD) && $(RV_CC) -std=c11 -march=rv32imac -mabi=ilp32 -Wall -Wextra -Werror \
	    -ffreestanding -I../$(APP_DIR)/include -c $(addprefix ../,$(APP_SRC))); \
	fi

cli: ## build host CLI (signing / header builder / OTA sender)
	@if [ -f $(CLI_DIR)/Cargo.toml ]; then echo "[cli] cargo build"; \
	  $(CARGO) build --release --manifest-path $(CLI_DIR)/Cargo.toml; \
	else echo "[skip] cli: $(CLI_DIR)/Cargo.toml not present yet"; fi

bridge: dirs ## build GHDL <-> Renode socket bridge
	@if [ -f $(BRIDGE_DIR)/Makefile ]; then echo "[bridge] make"; \
	  $(MAKE) -C $(BRIDGE_DIR) OUT=$(CURDIR)/$(BUILD)/ghdl_renode_socket_bridge; \
	else echo "[skip] bridge: $(BRIDGE_DIR)/Makefile not present yet"; fi

# ---- Verification (mirrors VTP-001 steps 1-4) -------------------------------
test: test-hdl prove test-bl2 kani fuzz-app ## run all verification steps

test-hdl: hdl ## VTP-001 step 1: GHDL testbench + waveform
	@if [ -z "$(HDL_TB)" ]; then echo "[skip] test-hdl: no testbench yet"; else \
	  $(GHDL) -e --std=$(GHDL_STD) --workdir=$(GHDL_WORK) -o $(BUILD)/$(HDL_TOP) $(HDL_TOP); \
	  $(GHDL) -r --std=$(GHDL_STD) --workdir=$(GHDL_WORK) $(HDL_TOP) \
	    --assert-level=error --wave=$(BUILD)/rot_tb.ghw; \
	fi

prove: ## VTP-001 step 2: GNATprove on BL1
	@if [ -f $(BL1_GPR) ]; then \
	  cd $(BL1_DIR) && $(GNATPROVE) -P bl1.gpr --mode=all --level=4 --report=all; \
	else echo "[skip] prove: $(BL1_GPR) not present yet"; fi

test-bl2: ## VTP-001 step 3a: rust unit tests on the host
	@if [ -f $(BL2_DIR)/Cargo.toml ]; then \
	  cd $(BL2_DIR) && $(CARGO) test --target x86_64-unknown-linux-gnu --features crypto; \
	else echo "[skip] test-bl2: $(BL2_DIR)/Cargo.toml not present yet"; fi

kani: ## VTP-001 step 3b: kani model checking of the header parser
	@if [ -f $(BL2_DIR)/Cargo.toml ]; then \
	  cd $(BL2_DIR) && $(CARGO) kani --harness verify_header_parsing; \
	else echo "[skip] kani: $(BL2_DIR)/Cargo.toml not present yet"; fi

fuzz-app: dirs ## VTP-001 step 4: ASan/UBSan harness for the OTA parser
	@if [ -f $(APP_DIR)/src/ota_parser.c ] && [ -f $(APP_DIR)/test/fuzz_harness.c ]; then \
	  $(HOST_CC) -fsanitize=address,undefined -Wall -Wextra -Werror -I$(APP_DIR)/include \
	    $(APP_DIR)/src/ota_parser.c $(APP_DIR)/test/fuzz_harness.c -o $(BUILD)/fuzzer; \
	  for f in $(APP_DIR)/corpus/*.bin; do [ -e "$$f" ] && $(BUILD)/fuzzer < "$$f"; done || true; \
	else echo "[skip] fuzz-app: ota_parser.c / fuzz_harness.c not present yet"; fi

# ---- Co-simulation (VTP-001 step 5) -----------------------------------------
cosim-bridge: ## terminal 1: start the GHDL IPC bridge
	@if [ -x $(BUILD)/ghdl_renode_socket_bridge ]; then \
	  $(BUILD)/ghdl_renode_socket_bridge --port $(BRIDGE_PORT); \
	else echo "[skip] cosim-bridge: run 'make bridge' first (not implemented yet)"; fi

cosim: ## terminal 2: launch the Renode platform script
	@if [ -f $(RESC) ]; then $(RENODE) -e "s @$(RESC); start"; \
	else echo "[skip] cosim: $(RESC) not present yet"; fi

# ---- Housekeeping -----------------------------------------------------------
clean: ## remove build outputs
	rm -rf $(BUILD)/*
	@touch $(BUILD)/.gitkeep 2>/dev/null || true

distclean: clean ## also remove cargo/gnat artifacts
	rm -rf $(BL2_DIR)/target $(BL2_DIR)/entry/target $(CLI_DIR)/target $(BL1_DIR)/obj


# ---- Spec validation --------------------------------------------------------
.PHONY: spec-check
spec-check: ## validate the shared image header spec
	@python3 scripts/check_spec.py


# ---- CLI tests --------------------------------------------------------------
.PHONY: test-cli
test-cli: ## run cerberus-cli unit tests
	@if [ -f $(CLI_DIR)/Cargo.toml ]; then $(CARGO) test --manifest-path $(CLI_DIR)/Cargo.toml; \
	else echo "[skip] test-cli: $(CLI_DIR)/Cargo.toml not present yet"; fi


# ---- Dev signing keys -------------------------------------------------------
CLI_BIN := $(CLI_DIR)/target/release/cerberus-cli

sign-keys: cli ## generate dev signing keys in keys/ (skips if present)
	@if [ -f keys/dev.key ]; then echo "[skip] keys/dev.key already exists"; \
	else $(CLI_BIN) keygen keys/dev; fi


# ---- Test vectors -----------------------------------------------------------
VEC_DIR := test/vectors

.PHONY: vectors vectors-check
vectors: cli ## regenerate test/vectors (deterministic)
	@rm -f $(VEC_DIR)/*.img $(VEC_DIR)/manifest.toml
	@$(CLI_BIN) vectors $(VEC_DIR)

vectors-check: cli ## fail if test/vectors differs from a fresh generation
	@rm -rf $(BUILD)/vectors-check
	@$(CLI_BIN) vectors $(BUILD)/vectors-check > /dev/null
	@diff -r -x .gitkeep -x README.md $(BUILD)/vectors-check $(VEC_DIR) && echo "[ok] test/vectors is up to date"

# ---- Spec validation --------------------------------------------------------
.PHONY: spec-check
spec-check: ## validate the image header and NV state specs
	@python3 scripts/check_spec.py
	@python3 scripts/check_nv_spec.py

# ---- BL2 bare-metal image ---------------------------------------------------
BL2_ENTRY    := $(BL2_DIR)/entry
BL2_ELF      := $(BL2_ENTRY)/target/$(RV_TARGET)/release/bl2
RUST_SYSROOT := $(shell rustc --print sysroot 2>/dev/null)
LLVM_BIN     := $(RUST_SYSROOT)/lib/rustlib/x86_64-unknown-linux-gnu/bin

.PHONY: bl2-entry bl2-image
bl2-entry: ## build the BL2 bare-metal ELF (RV32IMAC)
	@$(CARGO) build --release --target $(RV_TARGET) --manifest-path $(BL2_ENTRY)/Cargo.toml
	@$(LLVM_BIN)/llvm-size $(BL2_ELF) 2>/dev/null || echo "(run 'rustup component add llvm-tools' for size output)"

bl2-image: bl2-entry cli ## sign the BL2 ELF into build/bl2.img (needs keys/dev.key)
	@test -f keys/dev.key || { echo "run 'make sign-keys' first"; exit 1; }
	@mkdir -p $(BUILD)
	@$(LLVM_BIN)/llvm-objcopy -O binary $(BL2_ELF) $(BUILD)/bl2.bin
	@echo "bl2.bin: $$(stat -c %s $(BUILD)/bl2.bin) bytes (partition limit 130816)"
	@python3 scripts/otp_words.py keys/dev.pub --bin $(BUILD)/bl2.bin
	@$(CLI_BIN) sign --key keys/dev.key --payload $(BUILD)/bl2.bin --out $(BUILD)/bl2.img --type bl2 --secver 1

.PHONY: otp-words
otp-words: ## print the OTP hash words for keys/dev.pub (for the VHDL generic)
	@python3 scripts/otp_words.py keys/dev.pub


# ---- Renode end-to-end run --------------------------------------------------
SCENARIO   ?= normal
RENODE_DIR := $(BUILD)/renode

.PHONY: renode-run
renode-run: bl2-image ## boot BL2 in Renode (SCENARIO=normal|update)
	@mkdir -p $(RENODE_DIR)
	@python3 scripts/renode_prepare.py --scenario $(SCENARIO) --cli $(CLI_BIN) --llvm-bin $(LLVM_BIN) --elf $(BL2_ELF)
	@$(RENODE) --disable-xwt --console --plain $(RENODE_DIR)/run.resc 2>&1 | tee $(RENODE_DIR)/run.log
	@python3 scripts/renode_check.py $(RENODE_DIR)/run.log $(RENODE_DIR)/expected.json


SCENARIOS := normal update tampered rollback wrong-otp unprovisioned bad-update fallback

.PHONY: renode-all
renode-all: bl2-image ## run every Renode scenario and summarize
	@mkdir -p $(RENODE_DIR); fail=0; \
	for s in $(SCENARIOS); do \
	  echo "=== scenario: $$s"; \
	  if python3 scripts/renode_prepare.py --scenario $$s --cli $(CLI_BIN) --llvm-bin $(LLVM_BIN) --elf $(BL2_ELF) > /dev/null \
	     && $(RENODE) --disable-xwt --console --plain $(RENODE_DIR)/run.resc > $(RENODE_DIR)/run-$$s.log 2>&1; then \
	    python3 scripts/renode_check.py $(RENODE_DIR)/run-$$s.log $(RENODE_DIR)/expected.json || fail=1; \
	  else \
	    echo "[FAIL] could not run scenario $$s"; fail=1; \
	  fi; \
	done; \
	if [ $$fail -eq 0 ]; then echo "[ok] all scenarios passed"; else echo "[FAIL] some scenarios failed"; exit 1; fi


# ---- GHDL-backed RoT (bridge) ---------------------------------------------
.PHONY: bridge-test cosim-run cosim-all
bridge-test: ## test the GHDL bridge and the VHDL RoT without Renode
	@python3 sim/bridge/test_bridge.py

cosim-run: bl2-image bridge ## boot BL2 in Renode with the VHDL RoT (SCENARIO=...)
	@python3 scripts/renode_run.py --rot ghdl --scenario $(SCENARIO) --cli $(CLI_BIN) --llvm-bin $(LLVM_BIN) --elf $(BL2_ELF) --renode $(RENODE) --bridge $(BUILD)/ghdl_renode_socket_bridge --port $(BRIDGE_PORT)

cosim-all: bl2-image bridge ## run every scenario against the VHDL RoT
	@fail=0; \
	for s in $(SCENARIOS); do \
	  echo "=== scenario: $$s (VHDL RoT)"; \
	  python3 scripts/renode_run.py --rot ghdl --scenario $$s --cli $(CLI_BIN) --llvm-bin $(LLVM_BIN) --elf $(BL2_ELF) --renode $(RENODE) --bridge $(BUILD)/ghdl_renode_socket_bridge --port $(BRIDGE_PORT) || fail=1; \
	done; \
	if [ $$fail -eq 0 ]; then echo "[ok] all scenarios passed against the VHDL RoT"; else echo "[FAIL] some scenarios failed"; exit 1; fi


# ---- BL1 tests --------------------------------------------------------------
.PHONY: test-bl1
test-bl1: bl1 ## BL1 header parser against the shared test vectors
	@python3 $(BL1_DIR)/tests/run_vectors.py $(BUILD)/bl1_check
	
# ---- BL1 on the RV32 target -------------------------------------------------
.PHONY: bl1-rv32 renode-bl1
bl1-rv32: ## build BL1 (Ada/SPARK) for RV32IMAC with the riscv64-elf GNAT
	@bash scripts/build_bl1_rv32.sh

renode-bl1: bl1 bl1-rv32 ## run BL1 in Renode against the shared vectors
	@python3 scripts/renode_bl1.py --renode $(RENODE)
	
.PHONY: test-sha256
test-sha256: bl1 ## SHA-256 in SPARK against hashlib and FIPS 180-4
	@python3 $(BL1_DIR)/tests/run_sha256.py $(BUILD)/sha256_check

.PHONY: test-ed25519
test-ed25519: bl1 ## Ed25519 in BL1 (Monocypher) against RFC 8032 and the vectors
	@python3 $(BL1_DIR)/tests/run_ed25519.py $(BUILD)/ed25519_check