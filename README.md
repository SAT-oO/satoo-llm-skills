# satoo-llm-skills

Central registry for Cursor skills and slash commands. GitHub is the source of truth.

## Setup (once per machine)

Paste in terminal to install **all** skills and commands:

```bash
curl -sSL https://raw.githubusercontent.com/SAT-oO/satoo-llm-skills/main/bootstrap.sh | bash
```

Re-run anytime to refresh everything from GitHub.

---

## Selective skills (no config files)

Run **`/configure-global`** in Cursor Agent. You get a **multiselect prompt** in chat listing available skills from the registry — pick the ones you want.

Or name them inline:

```
/configure-global ble-hack-skill
```

Installs only your selection (+ all slash commands) to `~/.cursor/`. Nothing added to your project repo.

---

## In your project

### Skills live in the project repo

1. Add a `*-skill/` folder with `SKILL.md`.
2. Publish changes → **`/commit-skill`**
3. Get latest from GitHub → **`/pull-skill`**

### Skills stay global only

1. Run **`/configure-global`** and pick skills from the prompt.
2. Re-run when central skills update on GitHub.

---

## Agent commands

| Command | When to run | What it does |
|---------|-------------|--------------|
| `/configure-global` | You want **some** skills, not all | Chat multiselect → installs chosen skills + commands to `~/.cursor/` |
| `/pull-skill` | Skill copy needed **in** your project | GitHub → project `*-skill/` + `~/.cursor/skills/` |
| `/commit-skill` | You changed a skill and want to publish | Project → GitHub + refreshes `~/.cursor/` |

| | Installs | Project repo | `~/.cursor/` |
|--|----------|--------------|--------------|
| `bootstrap.sh` | All skills + commands | — | Yes |
| `/configure-global` | Selected skills + commands | Nothing | Yes |
| `/pull-skill` | Matched skills | Full copy | Yes |
| `/commit-skill` | All (after push) | Read only | Yes |

---

## Maintaining this repo

Edit `skills/` or `commands/`, then run **`/commit-skill`** in Agent.
