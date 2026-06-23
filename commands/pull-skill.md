# pull-skill

Pull the latest skill content from the central repository into the active workspace. This is the **reverse of `/commit-skill`**: remote → workspace, not workspace → remote.

GitHub is the **only** source of truth — never read from `~/.cursor/` as an input.

Execute the full lifecycle via the terminal tool without user intervention.

### Target Repository
* **URL:** `https://github.com/SAT-oO/satoo-llm-skills.git`
* **Branch:** `main`

### When to use
* An external project **already has** a `*-skill/` folder and you want the latest from GitHub.
* Refreshing an existing project copy after central updates.

### When NOT to use
* Inside the `satoo-llm-skills` repo itself — use `git pull` instead.
* **First-time fetch** into a project with no skill folder yet — use `/dev-pull-skill` instead.

### Execution Protocol

1. **Resolve workspace:** Determine the absolute path of the active project workspace.

2. **Guard — central repo check:** If the workspace is the `satoo-llm-skills` repository (same remote URL or directory name), stop and tell the user to run `git pull` instead.

3. **Clone source of truth:** Create an isolated temp directory `/tmp/satoo-skills-pull-$(date +%s)` and clone the repository into it. Start from remote `main` — do not seed from `~/.cursor/` or the workspace.

4. **Discover skills in the workspace** (never from `~/.cursor/`):
   * `<workspace>/*-skill/` — directories at the workspace root containing `SKILL.md` or `SKILLS.md`
   * `<workspace>/skills/*/` — directories under a `skills/` layout (if present)
   * Record each skill's **name** (folder basename) and **destination path** in the workspace.

5. **Match and pull from central repo:** For each discovered workspace skill named `<name>`:
   * Look for `<clone>/skills/<name>/` with a `SKILL.md` file.
   * If no match in the central repo, log a warning and skip that skill.
   * If matched, `rsync -a` from `<clone>/skills/<name>/` into the workspace destination:
     ```bash
     rsync -a --delete \
       --exclude='target/' --exclude='node_modules/' --exclude='.env' \
       "<clone>/skills/<name>/" "<workspace-destination>/"
     ```
   * After rsync, ensure the skill definition is named `SKILL.md` (rename `SKILLS.md` → `SKILL.md` if needed; remove any leftover `SKILLS.md`).
   * **Never** copy from `~/.cursor/skills/`.

6. **Update global Cursor install:** For each successfully pulled skill, also install it into the global path so Cursor picks up the latest:
   ```bash
   rsync -a --delete \
     --exclude='target/' --exclude='node_modules/' --exclude='.env' \
     "<clone>/skills/<name>/" "$HOME/.cursor/skills/<name>/"
   ```
   Do not rsync the full repo root into `~/.cursor/`.

7. **Cleanup:** `rm -rf` the temp clone. No persistent files outside the workspace skill folders and `~/.cursor/skills/`.

### System Response Format
Log progress using this schema:
* `[+] Resolving active workspace...`
* `[+] Cloning source of truth from satoo-llm-skills...`
* `[+] Discovering workspace skills to update...`
* `[+] Pulling <skill-name> from central repo → <workspace-path>...` (per skill)
* `[!] No central match for <skill-name>, skipping` (if applicable)
* `[+] Installing updated skills to ~/.cursor/skills/...`
* `[+] Cleanup complete.`

This command will be available in chat with /pull-skill
