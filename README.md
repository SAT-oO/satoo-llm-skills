# satoo-llm-skills

Central registry for Cursor skills and slash commands. GitHub is the source of truth.

## Setup (two steps)

**1. Terminal — install slash commands (once per machine):**

```bash
curl -sSL https://raw.githubusercontent.com/SAT-oO/satoo-llm-skills/main/bootstrap.sh | bash
```

**2. Cursor Agent — install skills you need:**

Run **`/configure-global`** and pick from the multiselect prompt.

Or name them inline: `/configure-global ble-hack-skill`

Re-run bootstrap to refresh commands. Re-run `/configure-global` to refresh skills.

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
| `/configure-global` | After bootstrap, to install skills | Chat multiselect → chosen skills to `~/.cursor/skills/` |
| `/pull-skill` | Skill copy needed **in** your project | GitHub → project `*-skill/` + `~/.cursor/skills/` |
| `/commit-skill` | You changed a skill and want to publish | Project → GitHub + refresh commands & published skills |

| | Commands | Skills | Project repo |
|--|----------|--------|--------------|
| `bootstrap.sh` | Install all | — | — |
| `/configure-global` | — | Install selected | — |
| `/pull-skill` | — | Install matched | Full copy |
| `/commit-skill` | Refresh all | Install published | Read only |

---

## Maintaining this repo

Edit `skills/` or `commands/`, then run **`/commit-skill`** in Agent.
