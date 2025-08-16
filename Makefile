# Makefile for cc-statusline-rs installation

# Installation directory
INSTALL_DIR := $(HOME)/.claude
BINARY_NAME := statusline
TARGET_PATH := $(INSTALL_DIR)/cc-statusline-rs
SETTINGS_FILE := $(INSTALL_DIR)/settings.json

# Default target
.PHONY: install
install: build
	@echo "Installing cc-statusline-rs to $(TARGET_PATH)..."
	@mkdir -p $(INSTALL_DIR)
	@cp target/release/$(BINARY_NAME) $(TARGET_PATH)
	@chmod +x $(TARGET_PATH)
	@echo "Updating settings.json..."
	@if [ -f "$(SETTINGS_FILE)" ]; then \
		python3 -c "import json; \
		data = json.load(open('$(SETTINGS_FILE)')); \
		data['statusLine'] = {'type': 'command', 'command': '~/.claude/cc-statusline-rs'}; \
		json.dump(data, open('$(SETTINGS_FILE)', 'w'), indent=2)" && \
		echo "✓ Updated existing settings.json"; \
	else \
		echo '{"statusLine": {"type": "command", "command": "~/.claude/cc-statusline-rs"}}' > "$(SETTINGS_FILE)" && \
		echo "✓ Created new settings.json"; \
	fi
	@echo "✓ Installation complete!"
	@echo ""
	@echo "Next steps:"
	@echo "1. Restart Claude Code to use the new statusline"

# Build the release binary
.PHONY: build
build:
	@echo "Building release binary..."
	@cargo build --release

.PHONY: check
check:
	@cargo check
	@cargo clippy
	@cargo fmt --check

.PHONY: fmt
fmt:
	@cargo fmt

.PHONY: test
test: build
	@echo "Running tests with sample data..."
	@cargo run < test.json

.PHONY: clean
clean:
	@cargo clean

.DEFAULT_GOAL := help
