GIT_TAG := $(shell git describe --tags --candidates 1)
BIN_DIR = "bin"
X86_64_TAG = "x86_64-unknown-linux-gnu"
BUILD_PATH_X86_64 = "target/$(X86_64_TAG)/release"
AARCH64_TAG = "aarch64-unknown-linux-gnu"
BUILD_PATH_AARCH64 = "target/$(AARCH64_TAG)/release"

.PHONY: all

install:
	cargo install --path .

build-x86_64:
	cargo build --profile highperf --target $(X86_64_TAG) --target-dir $(BIN_DIR)

build-aarch64:
	cargo build --profile highperf --target $(AARCH64_TAG) --target-dir $(BIN_DIR)

build:
	RUSTFLAGS="-C target-cpu=native" cargo build --profile highperf --target-dir $(BIN_DIR)