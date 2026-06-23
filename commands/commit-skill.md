# commit-skill

Publish skill changes to the central repository and reinstall from GitHub. The remote repo is the **only** source of truth — never read from or write to `~/.cursor/` as an input.

Execute the full lifecycle via the terminal tool without user intervention.

### Target Repository
* **URL:** `https://github.com/SAT-oO/satoo-llm-skills.git`
* **Branch:** `main`

### Execution Protocol

1. **Resolve workspace:** Determine the absolute path of the active project workspace.

2. **Clone source of truth:** Create an isolated temp directory `/tmp/satoo-skills-sync-$(date +%s)` and clone the repository into it. Start every run from remote `main` — do not seed the clone from `~/.cursor/` or any other local mirror.

3. **Discover skills to publish** (inputs come from the workspace only, never from `~/.cursor/`):
   * `<workspace>/skills/*/` — directories under a `skills/` layout
   * `<workspace>/*-skill/` — directories at the workspace root (project-embedded skills)
   * A folder qualifies only if it contains a skill definition file. **Canonical name is `SKILL.md`** (singular) per Cursor convention — not `SKILLS.md`.
   * If a folder has `SKILLS.md` but no `SKILL.md`, rename `SKILLS.md` → `SKILL.md` before merging (treat as a migration, not a second file).
   * If a folder has both, keep only `SKILL.md` and delete `SKILLS.md` from the merged copy.
   * Exclude build artifacts: `target/`, `node_modules/`, `.env`, `*.lock` (unless the skill genuinely needs them, e.g. `Cargo.lock` for Rust binaries)

4. **Merge into the clone:**
   * `mkdir -p skills` inside the clone if needed.
   * `rsync -a` each discovered skill folder into `<clone>/skills/<name>/` (no `--delete` on this step — only add or update contributed skills).
   * After each rsync, verify `<clone>/skills/<name>/SKILL.md` exists. If only `SKILLS.md` slipped through, rename it to `SKILL.md`.
   * Remove any `SKILLS.md` left in `<clone>/skills/` — the repository must not contain `SKILLS.md` files.
   * If the workspace contains a `commands/` directory (maintainers editing this repo), `rsync -a` it into `<clone>/commands/`. This is how `commit-skill.md` self-updates: edits land in the repo, not in a local mirror.
   * **Never** copy from `~/.cursor/commands/` or `~/.cursor/skills/`.

5. **Publish to GitHub:** Inside the clone, run `git status --porcelain`.
   * If there are changes: `git add -A`, commit with `Automated sync: YYYY-MM-DD HH:MM:SS from <workspace-name>`, push to `origin main`.
   * If no changes: skip commit and push.

6. **Install from repository:** After push (or if already up to date), install **only** `commands/` and `skills/` from the clone into the global Cursor path:
   ```bash
   rsync -a --delete "$CLONE/commands/" "$HOME/.cursor/commands/"
   rsync -a --delete "$CLONE/skills/" "$HOME/.cursor/skills/"
   ```
   Do not rsync the full repo root into `~/.cursor/`.

7. **Cleanup:** `rm -rf` the temp clone.

### System Response Format
Log progress using this schema:
* `[+] Resolving active workspace...`
* `[+] Cloning source of truth from satoo-llm-skills...`
* `[+] Discovering workspace skills to publish (SKILL.md convention)...`
* `[+] Normalizing skill filenames (SKILLS.md → SKILL.md if needed)...`
* `[+] Merging skills (and commands, if present) into repository tree...`
* `[+] Pushing to GitHub...` (or `[=] No changes to push`)
* `[+] Installing commands/ and skills/ from repository to ~/.cursor/...`
* `[+] Cleanup complete.`

This command will be available in chat with /commit-skill
