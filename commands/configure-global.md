# configure-global

Install selected skills from the central repository into `~/.cursor/skills/`. **Does not install slash commands** — run `bootstrap.sh` first for those.

GitHub is the **only** source of truth — never read from `~/.cursor/` as an input. No skill content is copied into the project repo.

### Target Repository
* **URL:** `https://github.com/SAT-oO/satoo-llm-skills.git`
* **Branch:** `main`

### Prerequisites
* `bootstrap.sh` has been run (slash commands installed to `~/.cursor/commands/`).

### When to use
* After bootstrap, to install the skills you need for this project.
* To refresh selected skills after updates on GitHub.

### When NOT to use
* Inside the `satoo-llm-skills` repo — run `bootstrap.sh` or `git pull` instead.
* You want skill files **inside** the project repo — use `/pull-skill` instead.
* Slash commands are missing — run `bootstrap.sh` first.

### How skill selection works (no config files)

**1. Chat multiselect (default)** — After listing available skills from the central repo, use the **AskQuestion** tool with `allow_multiple: true` to present every skill in `skills/` as options. Wait for the user's selection before installing.

**2. Inline in the user's message (skip prompt)** — If the user already named skills (e.g. `/configure-global ble-hack-skill`), use those names directly.

If the user selects nothing and named nothing, stop and ask which skills they want.

### Execution Protocol

1. **Resolve workspace:** Determine the absolute path of the active project workspace.

2. **Guard — central repo check:** If the workspace is the `satoo-llm-skills` repository, stop and tell the user to run `bootstrap.sh` instead.

3. **Clone source of truth:** Create `/tmp/satoo-skills-configure-$(date +%s)` and clone from remote `main`. Do not seed from `~/.cursor/`.

4. **List available skills:** Enumerate directories under `<clone>/skills/` that contain `SKILL.md`.

5. **Resolve user selection:**
   * If the user's message already names specific skills → use those (validate against step 4).
   * Otherwise → **AskQuestion** multiselect: one option per available skill, `allow_multiple: true`. Do not proceed until the user answers.

6. **Install selected skills only:** For each chosen skill name `<name>`:
   ```bash
   mkdir -p "$HOME/.cursor/skills"
   rsync -a --delete \
     --exclude='target/' --exclude='node_modules/' --exclude='.env' \
     "$CLONE/skills/<name>/" "$HOME/.cursor/skills/<name>/"
   ```
   Ensure `SKILL.md` exists (rename `SKILLS.md` → `SKILL.md` if needed).

7. **Do not modify the project repo:** No skill files or folders written to the workspace.

8. **Cleanup:** `rm -rf` the temp clone.

### Command comparison

| Command | Installs commands | Installs skills | Writes to project? |
|---------|-------------------|-----------------|-------------------|
| `bootstrap.sh` | All | No | No |
| `/configure-global` | No | Selected (user picks) | No |
| `/pull-skill` | No | Matched from project | Yes |
| `/commit-skill` | All (refresh) | Published only | No (reads project) |

### System Response Format
* `[+] Resolving active workspace...`
* `[+] Cloning source of truth from satoo-llm-skills...`
* `[+] Available skills: <name1>, <name2>, ...`
* `[+] Prompting skill selection...` (or `[+] Using skills from user message: ...`)
* `[+] Installing <skill-name> to ~/.cursor/skills/...` (per skill)
* `[+] Cleanup complete.`

This command will be available in chat with /configure-global
