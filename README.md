# satoo-llm-skills

Central registry for Cursor skills and slash commands. GitHub is the source of truth.

## Setup (once per machine)

Paste in terminal:

```bash
curl -sSL https://raw.githubusercontent.com/SAT-oO/satoo-llm-skills/main/bootstrap.sh | bash
```

Installs slash commands and skills into `~/.cursor/`. Re-run anytime to refresh from GitHub.

---

## In your project

### Option A — skills live in the project repo

1. Add a `*-skill/` folder with `SKILL.md` (e.g. `ble-hack-skill/SKILL.md`).
2. In Cursor Agent, run **`/configure-global`** on first open (installs matching skills globally).
3. To publish changes → **`/commit-skill`**
4. To get latest from GitHub → **`/pull-skill`**

### Option B — no skill files in the project repo

1. Add `.cursor/satoo-skills.json`:
   ```json
   { "skills": ["ble-hack-skill"] }
   ```
2. In Cursor Agent, run **`/configure-global`**
3. Re-run **`/configure-global`** when central skills update on GitHub.

---

## Agent commands

| Command | When to run | What it does |
|---------|-------------|--------------|
| `/configure-global` | New project, or after central skills update | Installs named skills + all commands to `~/.cursor/`. **No skill files added to your project.** |
| `/pull-skill` | You want the latest skill copy **inside** your project | Pulls from GitHub → project `*-skill/` folder + `~/.cursor/skills/` |
| `/commit-skill` | You changed a skill and want to publish | Pushes project `*-skill/` → GitHub + refreshes `~/.cursor/` |

| | Project repo | `~/.cursor/` | GitHub |
|--|--------------|--------------|--------|
| `/configure-global` | Manifest only | Install | Read |
| `/pull-skill` | Full skill copy | Install | Read |
| `/commit-skill` | Read skill | Install | Write |

---

## Maintaining this repo

Edit `skills/` or `commands/`, then run **`/commit-skill`** in Agent.
