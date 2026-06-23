# dev: pull-skill

Fetch skill packages from the central repository into the **external project folder** for local development and editing. Prompts the user to choose which skills to copy.

GitHub is the **only** source of truth — never read from `~/.cursor/` as an input.

### Target Repository
* **URL:** `https://github.com/SAT-oO/satoo-llm-skills.git`
* **Branch:** `main`

### When to use
* Starting skill development in an external project — you need a local `*-skill/` copy to edit.
* Bootstrapping a project repo with one or more skills from the registry (replaces manual `git clone` + `rsync`).

### When NOT to use
* Inside the `satoo-llm-skills` repo — clone or `git pull` that repo directly.
* Updating skills that **already exist** in the project — use `/pull-skill` instead (discovers existing folders).
* Skills should stay global only (no project copy) — use `/configure-skills` instead.

### Difference from `/pull-skill`

| | `/dev-pull-skill` | `/pull-skill` |
|--|-------------------|---------------|
| Skill selection | User picks from registry (chat multiselect) | Auto-discovers existing `*-skill/` folders |
| Creates project copy | **Yes** — writes `<workspace>/<name>/` | Updates existing paths only |
| Use case | First-time fetch for development | Refresh existing project copies |

### How skill selection works

**1. Chat multiselect (default)** — List skills from `<clone>/skills/`, then **AskQuestion** with `allow_multiple: true`.

**2. Inline in the user's message** — e.g. `/dev-pull-skill ble-hack-skill` skips the prompt.

### Execution Protocol

1. **Resolve workspace:** Determine the absolute path of the active project workspace.

2. **Guard — central repo check:** If the workspace is `satoo-llm-skills`, stop and tell the user to work in that repo directly.

3. **Clone source of truth:** Create `/tmp/satoo-skills-dev-pull-$(date +%s)` and clone from remote `main`.

4. **List available skills:** Directories under `<clone>/skills/` containing `SKILL.md`.

5. **Resolve user selection:** Named in message, or **AskQuestion** multiselect. Do not proceed without a selection.

6. **Copy into project** — for each chosen skill `<name>`:
   ```bash
   mkdir -p "<workspace>/<name>"
   rsync -a --delete \
     --exclude='target/' --exclude='node_modules/' --exclude='.env' \
     "<clone>/skills/<name>/" "<workspace>/<name>/"
   ```
   Destination is always `<workspace>/<name>/` at the project root (e.g. `ble-hack-skill/`).
   Ensure `SKILL.md` exists (rename `SKILLS.md` → `SKILL.md`; remove leftover `SKILLS.md`).

7. **Update global install** — for each skill, also sync to Cursor global path:
   ```bash
   rsync -a --delete \
     --exclude='target/' --exclude='node_modules/' --exclude='.env' \
     "<clone>/skills/<name>/" "$HOME/.cursor/skills/<name>/"
   ```

8. **Cleanup:** `rm -rf` the temp clone.

### System Response Format
* `[+] Resolving active workspace...`
* `[+] Cloning source of truth from satoo-llm-skills...`
* `[+] Available skills: <name1>, <name2>, ...`
* `[+] Prompting skill selection...` (or `[+] Using skills from user message: ...`)
* `[+] Copying <skill-name> → <workspace>/<skill-name>/...` (per skill)
* `[+] Installing <skill-name> to ~/.cursor/skills/...` (per skill)
* `[+] Cleanup complete.`

This command will be available in chat with /dev-pull-skill
