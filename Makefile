# Derived values (DO NOT TOUCH).
CURRENT_MAKEFILE_PATH := $(abspath $(lastword $(MAKEFILE_LIST)))
CURRENT_MAKEFILE_DIR := $(patsubst %/,%,$(dir $(CURRENT_MAKEFILE_PATH)))

.DEFAULT_GOAL := help

# Installation directory
INSTALL_DIR := $(HOME)/.claude
BINARY_NAME := statusline
TARGET_PATH := $(INSTALL_DIR)/cc-statusline-rs
SETTINGS_FILE := $(INSTALL_DIR)/settings.json

install: build # Build and add the status line to Claude Code
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

build: # Build the release binary
	@echo "Building release binary..."
	@cargo build --release

check: # cargo check, clippy, and fmt
	@cargo check
	@cargo clippy
	@cargo fmt --check

fmt: # cargo fmt
	@cargo fmt

test: build # test the status line with sample data
	@echo "Running tests with sample data..."
	@cargo run < test.json

clean: # clean the project
	@cargo clean

help: # Display this help
	@-+echo "Run make with one of the following targets:"
	@-+echo
	@-+grep -Eh "^[a-z-]+:.*#" $(CURRENT_MAKEFILE_PATH) | sed -E 's/^(.*:)(.*#+)(.*)/  \1 @@@ \3 /' | column -t -s "@@@"

.PHONY: help install build check fmt test clean