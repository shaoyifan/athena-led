BINARY_NAME:= athena-led
VERSION:= 1.0.0
OUTPUT_DIR:= ./dist
CARGO_FLAGS:= --release

ROOT_DIR = $(shell pwd)

ARM_TARGET:= aarch64-unknown-linux-musl
ARM_TARGET_SHORT:= aarch64-musl
X64_TARGET:= x86_64-unknown-linux-musl
X64_TARGET_SHORT:= x86_64-musl

.PHONY: all arm x64 check clean dirclean

all: arm x64 check

define do_build
	@echo "============================================="
	@echo "🚀 Building $(1)..."
	@echo "============================================="

	@cross build --target $(1) $(CARGO_FLAGS)

	@echo "============================================="
	@echo "📦 Packaging $(1)..."
	@echo "============================================="
	
	@mkdir -p $(OUTPUT_DIR)

	@TARGET_DIR="target/$(1)/release"; \
	TARGET_NAME="$(BINARY_NAME)-$(2)-v$(VERSION)"; \
	cd "$$TARGET_DIR" && \
	cp $(BINARY_NAME) "$$TARGET_NAME" && \
	tar -czf "$$TARGET_NAME.tar.gz" "$(BINARY_NAME)" && \
	sha256sum "$$TARGET_NAME.tar.gz" > "$$TARGET_NAME.tar.gz.sha256" && \
	mv "$$TARGET_NAME" "$(ROOT_DIR)/$(OUTPUT_DIR)/" && \
	mv "$$TARGET_NAME.tar.gz" "$(ROOT_DIR)/$(OUTPUT_DIR)/" && \
	mv "$$TARGET_NAME.tar.gz.sha256" "$(ROOT_DIR)/$(OUTPUT_DIR)/"

	@cd $(ROOT_DIR)
	@printf "    \033[32mPackage:\033[0m $(OUTPUT_DIR)/$(BINARY_NAME)-$(2)-v$(VERSION).tar.gz\n"
	@printf "    \033[32mSHA256:\033[0m %s\n" "$$(cut -d' ' -f1 < $(OUTPUT_DIR)/$(BINARY_NAME)-$(2)-v$(VERSION).tar.gz.sha256)"
endef

arm:
	$(call do_build,$(ARM_TARGET),$(ARM_TARGET_SHORT))

x64:
	$(call do_build,$(X64_TARGET),$(X64_TARGET_SHORT))

check:
	@echo "============================================="
	@echo "📋 Testing generated files:"
	@echo "============================================="
	@file $(OUTPUT_DIR)/* 2>/dev/null | grep -v '\.tar\.gz' | grep -v '\.sha256' || true

clean:
	@cargo clean

dirclean:
	@cargo clean
	@rm -rfv $(OUTPUT_DIR)