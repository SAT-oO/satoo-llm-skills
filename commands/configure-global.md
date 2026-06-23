# configure-global

Configure global Cursor skills and commands for the active external project **without copying skill content into the project repo**.

GitHub is the **only** source of truth — never read from `~/.cursor/` as an input. All skill binaries and `SKILL.md` files are installed only under `~/.cursor/`.

### Target Repository
* **URL:** `https://github.com/SAT-oO/satoo-llm-skills.git`
* **Branch:** `main`

### When to use
* You want **selective** skills installed globally (not the full registry from `bootstrap.sh`).
* Your project should stay lean — no `*-skill` folders or `SKILL.md` copies in the repo.

### When NOT to use
* Inside the `satoo-llm-skills` repo — run `bootstrap.sh` or `git pull` instead.
* You want skill files **inside** the project repo — use `/pull-skill` instead.
* You want **all** skills — run `bootstrap.sh` instead.

### How skill selection works (no config files)

Do **not** create a manifest or JSON file. Selection happens at runtime:

**1. Chat multiselect (default)** — After listing available skills from the central repo, use the **AskQuestion** tool with `allow_multiple: true` to present every skill in `skills/` as options. Wait for the user's selection before installing.

**2. Inline in the user's message (skip prompt)** — If the user already named skills in the same message (e.g. `/configure-global ble-hack-skill` or "install ble-hack-skill and my-database-skill"), use those names directly. Match names to folders under `skills/` in the central repo.

If the user selects nothing and named nothing, stop and ask which skills they want.

### Execution Protocol

1. **Resolve workspace:** Determine the absolute path of the active project workspace.

2. **Guard — central repo check:** If the workspace is the `satoo-llm-skills` repository, stop and tell the user to run `bootstrap.sh` instead.

3. **Clone source of truth:** Create `/tmp/satoo-skills-configure-$(date +%s)` and clone from remote `main`. Do not seed from `~/.cursor/`.

4. **List available skills:** Enumerate directories under `<clone>/skills/` that contain `SKILL.md`. These are the only valid options.

5. **Resolve user selection:**
   * If the user's message already names specific skills → use those (validate against step 4).
   * Otherwise → **AskQuestion** multiselect: one option per available skill, `allow_multiple: true`. Do not proceed until the user answers.

6. **Install global commands:**
   ```bash
   rsync -a --delete "$CLONE/commands/" "$HOME/.cursor/commands/"
   ```

7. **Install selected skills only:** For each chosen skill name `<name>`:
   ```bash
   rsync -a --delete \
     --exclude='target/' --exclude='node_modules/' --exclude='.env' \
     "$CLONE/skills/<name>/" "$HOME/.cursor/skills/<name>/"
   ```
   Ensure `SKILL.md` exists (rename `SKILLS.md` → `SKILL.md` if needed).

8. **Do not modify the project repo:** No skill files, no manifest, no `*-skill/` folders written to the workspace.

9. **Cleanup:** `rm -rf` the temp clone.

### Command comparison

| Command | Skill selection | Writes to project? | Writes to `~/.cursor/`? |
|---------|-----------------|--------------------|-------------------------|
| `bootstrap.sh` | All skills | No | Yes |
| `/configure-global` | User picks (chat multiselect) | No | Yes (selected only) |
| `/pull-skill` | Matched from project folders | Yes | Yes |
| `/commit-skill` | From project folders | No (reads project) | Yes |

### System Response Format
* `[+] Resolving active workspace...`
* `[+] Cloning source of truth from satoo-llm-skills...`
* `[+] Available skills: <name1>, <name2>, ...`
* `[+] Prompting skill selection...` (or `[+] Using skills from user message: ...`)
* `[+] Installing global commands to ~/.cursor/commands/...`
* `[+] Installing <skill-name> to ~/.cursor/skills/...` (per skill)
* `[+] Cleanup complete.`

This command will be available in chat with /configure-global
