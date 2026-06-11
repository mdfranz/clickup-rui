# ==============================================================================
# Makefile for clickup-rui
# A high-performance, premium TUI/CLI client for ClickUp in Rust.
# ==============================================================================

# Executables
CARGO := cargo
RM    := rm -rf

# Project Metadata
BINARY_NAME := clickup-rui
TARGET_DIR  := target
RELEASE_BIN := $(TARGET_DIR)/release/$(BINARY_NAME)
DEBUG_BIN   := $(TARGET_DIR)/debug/$(BINARY_NAME)
# Installation Paths
PREFIX      ?= $(HOME)/.local
INSTALL_DIR := $(PREFIX)/bin

# Style / Colors
YELLOW := \033[33m
BLUE   := \033[34m
GREEN  := \033[32m
RED    := \033[31m
BOLD   := \033[1m
RESET  := \033[0m

.PHONY: all help build release run run-release check clippy fmt fmt-check test clean install uninstall setup browse menu doc

# Default target
all: help

##--- Help ---
## Prints this help menu with descriptions of all targets
help:
	@printf "$(BLUE)$(BOLD)clickup-rui⚡ Build & Development Tool$(RESET)\n"
	@printf "$(BLUE)==============================================$(RESET)\n"
	@printf "$(BOLD)Available targets:$(RESET)\n"
	@awk '/^##---/ { \
			section=$$0; \
			sub(/^##--- /, "", section); \
			sub(/ ---$$/, "", section); \
			printf "\n$(BOLD)$(YELLOW)%s$(RESET)\n", section; \
			next; \
		} \
		/^## / { \
			helpMsg=$$0; \
			sub(/^## /, "", helpMsg); \
			next; \
		} \
		/^[a-zA-Z_-]+:/ { \
			if (helpMsg != "") { \
				target=$$0; \
				sub(/:.*/, "", target); \
				printf "  $(GREEN)%-15s$(RESET) %s\n", target, helpMsg; \
				helpMsg=""; \
			} \
		}' $(MAKEFILE_LIST)
	@printf "\n"

##--- Development ---

## Build the binary in debug mode
build:
	@printf "$(BLUE)%-15s$(RESET) Building debug binary...\n" "Cargo"
	@$(CARGO) build
	@printf "$(GREEN)%-15s$(RESET) Debug build finished at $(DEBUG_BIN)\n" "Success"

## Build the binary in release mode (optimized)
release:
	@printf "$(BLUE)%-15s$(RESET) Building optimized release binary...\n" "Cargo"
	@$(CARGO) build --release
	@printf "$(GREEN)%-15s$(RESET) Release build finished at $(RELEASE_BIN)\n" "Success"

## Run cargo check on the workspace
check:
	@printf "$(BLUE)%-15s$(RESET) Checking codebase...\n" "Cargo"
	@$(CARGO) check

## Generate and open crate documentation
doc:
	@printf "$(BLUE)%-15s$(RESET) Generating documentation...\n" "Cargo"
	@$(CARGO) doc --no-deps --open

##--- Execution ---

## Run the debug binary
run:
	@printf "$(BLUE)%-15s$(RESET) Running debug binary...\n" "Cargo"
	@$(CARGO) run

## Run the optimized release binary
run-release:
	@printf "$(BLUE)%-15s$(RESET) Running release binary...\n" "Cargo"
	@$(CARGO) run --release

## Launch the ClickUp setup wizard
setup:
	@printf "$(BLUE)%-15s$(RESET) Launching ClickUp setup wizard...\n" "Cargo"
	@$(CARGO) run --release -- setup

## Launch the interactive dual-pane TUI browser
browse:
	@printf "$(BLUE)%-15s$(RESET) Launching dual-pane browser...\n" "Cargo"
	@$(CARGO) run --release -- browse

## Launch the main command picker menu
menu:
	@printf "$(BLUE)%-15s$(RESET) Launching TUI main menu...\n" "Cargo"
	@$(CARGO) run --release -- menu

##--- Quality Assurance ---

## Run the test suite
test:
	@printf "$(BLUE)%-15s$(RESET) Running all unit and integration tests...\n" "Cargo"
	@$(CARGO) test

## Run cargo clippy with strict warnings enabled
clippy:
	@printf "$(BLUE)%-15s$(RESET) Running Clippy lints...\n" "Cargo"
	@$(CARGO) clippy --all-targets --all-features -- -D warnings

## Format all rust source files
fmt:
	@printf "$(BLUE)%-15s$(RESET) Formatting Rust code...\n" "Cargo"
	@$(CARGO) fmt --all

## Check Rust code formatting without applying changes
fmt-check:
	@printf "$(BLUE)%-15s$(RESET) Checking Rust code format...\n" "Cargo"
	@$(CARGO) fmt --all -- --check

##--- Installation ---

## Install the binary locally into ~/.local/bin/
install: release
	@printf "$(BLUE)%-15s$(RESET) Installing binary to $(INSTALL_DIR)/...\n" "Install"
	@mkdir -p $(INSTALL_DIR)
	@cp $(RELEASE_BIN) $(INSTALL_DIR)/$(BINARY_NAME)
	@printf "$(GREEN)%-15s$(RESET) Installation complete!\n" "Success"

## Uninstall the binary from ~/.local/bin/
uninstall:
	@printf "$(BLUE)%-15s$(RESET) Uninstalling binary from $(INSTALL_DIR)/...\n" "Install"
	@rm -f $(INSTALL_DIR)/$(BINARY_NAME)
	@printf "$(GREEN)%-15s$(RESET) Uninstallation complete!\n" "Success"

## Clean build artifacts
clean:
	@printf "$(RED)%-15s$(RESET) Cleaning build artifacts...\n" "Cargo"
	@$(CARGO) clean
	@printf "$(GREEN)%-15s$(RESET) Clean complete!\n" "Success"
