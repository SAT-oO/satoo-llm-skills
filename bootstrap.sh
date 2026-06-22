#!/bin/bash
set -e

# --- CONFIGURATION ---
REPO_URL="https://github.com/SAT-oO/satoo-llm-skills.git"
TARGET_DIR="$HOME/.cursor"
TEMP_DIR="/tmp/satoo-skills-bootstrap-$(date +%s)"
# ---------------------

echo "==== Starting Cursor Skills Global Bootstrapper ===="
echo "[+] Target Directory: $TARGET_DIR"

# 1. Ensure target structures exist
mkdir -p "$TARGET_DIR/commands" "$TARGET_DIR/skills"

# 2. Clone the central repository cleanly to a temporary workspace
echo "[+] Fetching remote skill configurations from $REPO_URL..."
git clone --quiet "$REPO_URL" "$TEMP_DIR"

# 3. Synchronize skills and commands into the local profile path
echo "[+] Mirroring configurations to global path..."
# Using --delete ensures old or deleted remote skills are purged from the local machine profile
rsync -a --delete \
  --exclude='.git/' \
  --exclude='bootstrap.sh' \
  "$TEMP_DIR/" "$TARGET_DIR/"

# 4. Cleanup temporary files
echo "[+] Purging runtime cache..."
rm -rf "$TEMP_DIR"

echo "[+] Verification:"
echo "    -> Commands directory: $(ls -1 "$TARGET_DIR/commands" 2>/dev/null | wc -l) configuration files"
echo "    -> Skills directory:   $(ls -1 "$TARGET_DIR/skills" 2>/dev/null | wc -l) capability packages"
echo "==== Bootstrap Completed Successfully ===="