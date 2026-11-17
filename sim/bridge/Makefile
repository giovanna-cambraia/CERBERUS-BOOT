# Installs a launcher for the bridge (a Python script, nothing to compile).
OUT ?= ../../build/ghdl_renode_socket_bridge

.PHONY: all
all: $(OUT)

$(OUT): ghdl_renode_socket_bridge.py rot_bridge_top.vhd
	@mkdir -p $(dir $(OUT))
	@printf '#!/usr/bin/env bash\nexec python3 "%s" "$$@"\n' "$(abspath ghdl_renode_socket_bridge.py)" > $(OUT)
	@chmod +x $(OUT)
	@echo "[bridge] $(OUT)"