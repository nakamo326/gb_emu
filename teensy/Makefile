RUSTC         := $(shell rustup which rustc)
RUSTC_HOST    := $(shell $(RUSTC) -vV 2>&1 | sed -n 's/^host: //p')
RUSTC_SYSROOT := $(shell $(RUSTC) --print sysroot)
OBJCOPY       := $(RUSTC_SYSROOT)/lib/rustlib/$(RUSTC_HOST)/bin/rust-objcopy

CARGO      := cargo
TARGET     := thumbv7em-none-eabihf
TEENSY_MCU := TEENSY41
TEENSY_CLI := teensy_loader_cli

ELF := target/$(TARGET)/release/gb-teensy
HEX := target/$(TARGET)/release/gb-teensy.hex

.PHONY: build build-debug hex flash deploy clean

build:
	RUSTC="$(RUSTC)" $(CARGO) build --target $(TARGET) --release

build-debug:
	RUSTC="$(RUSTC)" $(CARGO) build --target $(TARGET)

hex: build
	$(OBJCOPY) -O ihex $(ELF) $(HEX)
	@echo "Generated: $(HEX)"

flash: hex
	$(TEENSY_CLI) --mcu=$(TEENSY_MCU) -w -v $(HEX)

deploy: flash

clean:
	$(CARGO) clean
