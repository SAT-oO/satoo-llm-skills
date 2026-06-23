#!/bin/bash
set -e

# --- CONFIGURATION ---
REPO_URL="https://github.com/SAT-oO/satoo-llm-skills.git"
TARGET_DIR="$HOME/.cursor"
TEMP_DIR="/tmp/satoo-skills-bootstrap-$(date +%s)"
# ---------------------

# Installs slash commands only. Skills are installed separately via /configure-global.

echo "==== Starting Cursor Skills Global Bootstrapper ===="
echo "[+] Install target: $TARGET_DIR/commands"
echo "[+] Source of truth: $REPO_URL"

mkdir -p "$TARGET_DIR/commands"

echo "[+] Fetching commands from remote..."
git clone --quiet --depth 1 --branch main --filter=blob:none --sparse "$REPO_URL" "$TEMP_DIR"
git -C "$TEMP_DIR" sparse-checkout set commands

echo "[+] Installing slash commands..."
rsync -a --delete "$TEMP_DIR/commands/" "$TARGET_DIR/commands/"

echo "[+] Removing temporary fetch..."
rm -rf "$TEMP_DIR"

echo "[+] Verification:"
echo "    -> Commands: $(ls -1 "$TARGET_DIR/commands" 2>/dev/null | wc -l | tr -d ' ') files"
echo "[+] Next step: run /configure-global in Cursor Agent to install skills."
echo "==== Bootstrap Completed Successfully ===="
