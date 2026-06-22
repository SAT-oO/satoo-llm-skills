#!/bin/bash
set -e

# --- CONFIGURATION ---
REPO_URL="https://github.com/SAT-oO/satoo-llm-skills.git"
TARGET_DIR="$HOME/.cursor"
TEMP_DIR="/tmp/satoo-skills-bootstrap-$(date +%s)"
# ---------------------

echo "==== Starting Cursor Skills Global Bootstrapper ===="
echo "[+] Target Directory: $TARGET_DIR"
echo "[+] Source of truth: $REPO_URL"

# 1. Ensure target structure exists
mkdir -p "$TARGET_DIR/commands" "$TARGET_DIR/skills"

# 2. Clone the central repository (remote is the only source of truth)
echo "[+] Fetching remote skill configurations..."
git clone --quiet --branch main "$REPO_URL" "$TEMP_DIR"

# 3. Install only commands/ and skills/ from the repo — nothing else
echo "[+] Installing commands from repository..."
rsync -a --delete "$TEMP_DIR/commands/" "$TARGET_DIR/commands/"

echo "[+] Installing skills from repository..."
rsync -a --delete "$TEMP_DIR/skills/" "$TARGET_DIR/skills/"

# 4. Cleanup
echo "[+] Purging temporary clone..."
rm -rf "$TEMP_DIR"

echo "[+] Verification:"
echo "    -> Commands: $(ls -1 "$TARGET_DIR/commands" 2>/dev/null | wc -l | tr -d ' ') files"
echo "    -> Skills:   $(ls -1 "$TARGET_DIR/skills" 2>/dev/null | wc -l | tr -d ' ') packages"
echo "==== Bootstrap Completed Successfully ===="
