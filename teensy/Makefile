RUSTC         := $(shell rustup which rustc)
RUSTC_HOST    := $(shell $(RUSTC) -vV 2>&1 | sed -n 's/^host: //p')
RUSTC_SYSROOT := $(shell $(RUSTC) --print sysroot)
OBJCOPY       := $(RUSTC_SYSROOT)/lib/rustlib/$(RUSTC_HOST)/bin/rust-objcopy

CARGO      := cargo
TARGET     := thumbv7em-none-eabihf
TEENSY_MCU := TEENSY41
# WSL からは Windows 側の teensy_loader_cli.exe を呼ぶことで USB 書き込みが安定する。
# 上書き例: make flash TEENSY_CLI="/mnt/c/Users/nakam/bin/teensy_loader_cli.exe"
TEENSY_CLI := teensy_loader_cli

ELF := target/$(TARGET)/release/gb-teensy
HEX := target/$(TARGET)/release/gb-teensy.hex

# 埋め込む ROM ファイル（絶対パスで渡す）。
# 上書き例: make ROM=../blargg/cgb-acid2.gbc build
ROM ?= $(abspath ../roms/game.gb)

.PHONY: build build-debug hex flash deploy clean

build:
	GB_ROM="$(ROM)" RUSTC="$(RUSTC)" $(CARGO) build --target $(TARGET) --release

build-debug:
	GB_ROM="$(ROM)" RUSTC="$(RUSTC)" $(CARGO) build --target $(TARGET)

hex: build
	$(OBJCOPY) -O ihex $(ELF) $(HEX)
	@echo "Generated: $(HEX)"

flash: hex
	@if echo "$(TEENSY_CLI)" | grep -qi '\.exe$$'; then \
		HEX_ARG="$$(wslpath -w '$(HEX)')"; \
	else \
		HEX_ARG="$(HEX)"; \
	fi; \
	echo "$(TEENSY_CLI) --mcu=$(TEENSY_MCU) -w -v $$HEX_ARG"; \
	$(TEENSY_CLI) --mcu=$(TEENSY_MCU) -w -v "$$HEX_ARG"

deploy: flash

clean:
	$(CARGO) clean
