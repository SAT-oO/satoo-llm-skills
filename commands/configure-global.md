# configure-global

Configure global Cursor skills and commands for the active external project **without copying skill content into the project repo**.

GitHub is the **only** source of truth — never read from `~/.cursor/` as an input. All skill binaries and `SKILL.md` files are installed only under `~/.cursor/`.

Execute the full lifecycle via the terminal tool without user intervention.

### Target Repository
* **URL:** `https://github.com/SAT-oO/satoo-llm-skills.git`
* **Branch:** `main`

### When to use
* In an **external project** that depends on central skills but should stay lean (no `*-skill` folders or `SKILL.md` copies in the repo).
* Onboarding a new machine or teammate — one command to wire up global Cursor from the central registry.
* After central skills are updated on GitHub and you want the latest globally without touching project files.

### When NOT to use
* Inside the `satoo-llm-skills` repo — run `bootstrap.sh` or `git pull` instead.
* When you want skill files **inside** the project repo — use `/pull-skill` instead.

### How the project declares its skills (no skill content in repo)

The project must name which central skills it needs. Check in this order:

1. **Manifest (preferred):** `.cursor/satoo-skills.json`
   ```json
   {
     "skills": ["ble-hack-skill"]
   }
   ```
2. **Fallback manifest:** `satoo-skills.json` at the workspace root (same schema).
3. **Name-only fallback:** `*-skill` directories at the workspace root — use the **folder name** as the skill identifier only. Do **not** read, copy, or update files inside those folders.

If no skills can be resolved, stop and ask the user to add `.cursor/satoo-skills.json`.

### Execution Protocol

1. **Resolve workspace:** Determine the absolute path of the active project workspace.

2. **Guard — central repo check:** If the workspace is the `satoo-llm-skills` repository, stop and tell the user to run `bootstrap.sh` instead.

3. **Resolve skill list:** Read `.cursor/satoo-skills.json`, then `satoo-skills.json`, then name-only scan of `*-skill/` folders at the workspace root. Collect unique skill names (e.g. `ble-hack-skill`).

4. **Clone source of truth:** Create `/tmp/satoo-skills-configure-$(date +%s)` and clone the repository from remote `main`. Do not seed from `~/.cursor/` or project skill folders.

5. **Install global commands:** Install all slash commands from the central repo:
   ```bash
   rsync -a --delete "$CLONE/commands/" "$HOME/.cursor/commands/"
   ```

6. **Install matching global skills:** For each resolved skill name `<name>`:
   * Verify `<clone>/skills/<name>/SKILL.md` exists.
   * If missing in central repo, log a warning and skip.
   * If present, install globally:
     ```bash
     rsync -a --delete \
       --exclude='target/' --exclude='node_modules/' --exclude='.env' \
       "$CLONE/skills/<name>/" "$HOME/.cursor/skills/<name>/"
     ```
   * Ensure `SKILL.md` exists in the global install (rename `SKILLS.md` → `SKILL.md` if needed; remove any `SKILLS.md`).

7. **Write project manifest (metadata only):** If `.cursor/satoo-skills.json` does not exist, create it with the resolved skill list. This file names skills only — **never** write `SKILL.md` or skill package files into the project repo.

8. **Do not modify project skill content:** Do not rsync skill folders into the workspace. Do not create `*-skill/` directories with skill files. The project repo stays free of skill payloads.

9. **Cleanup:** `rm -rf` the temp clone.

### Command comparison

| Command | Direction | Writes skill content to project? | Writes to `~/.cursor/`? |
|---------|-----------|----------------------------------|-------------------------|
| `/configure-global` | GitHub → global only | **No** | Yes (commands + named skills) |
| `/pull-skill` | GitHub → project + global | **Yes** | Yes |
| `/commit-skill` | Project → GitHub + global | No (reads from project) | Yes |

### System Response Format
Log progress using this schema:
* `[+] Resolving active workspace...`
* `[+] Reading skill manifest...`
* `[+] Skills to configure: <name1>, <name2>, ...`
* `[+] Cloning source of truth from satoo-llm-skills...`
* `[+] Installing global commands to ~/.cursor/commands/...`
* `[+] Installing <skill-name> to ~/.cursor/skills/...` (per skill)
* `[!] No central match for <skill-name>, skipping` (if applicable)
* `[+] Writing .cursor/satoo-skills.json manifest` (if created)
* `[+] Cleanup complete.`

This command will be available in chat with /configure-global
