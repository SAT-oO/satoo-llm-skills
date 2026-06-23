# satoo-llm-skills

Central registry for Cursor skills and slash commands. GitHub is the source of truth.

## Setup (two steps)

**1. Terminal — install slash commands (once per machine):**

```bash
curl -sSL https://raw.githubusercontent.com/SAT-oO/satoo-llm-skills/main/bootstrap.sh | bash
```

**2. Cursor Agent — install skills globally:**

Run **`/configure-skills`** and pick from the multiselect prompt.

Or: `/configure-skills ble-hack-skill`

---

## Skill development in an external project

**First time** — fetch a skill copy into your project to edit:

Run **`/dev-pull-skill`** and pick skill(s), or: `/dev-pull-skill ble-hack-skill`

Creates `<skill-name>/` in your project (e.g. `ble-hack-skill/SKILL.md`).

**After editing** — publish → **`/commit-skill`**

**Refresh existing copy** — **`/pull-skill`**

**Global only (no project copy)** — **`/configure-skills`**

---

## Agent commands

| Command | When to run | What it does |
|---------|-------------|--------------|
| `/configure-skills` | Skills global only, no project copy | Multiselect → `~/.cursor/skills/` |
| `/dev-pull-skill` | First-time fetch for editing in project | Multiselect → project `*-skill/` + `~/.cursor/skills/` |
| `/pull-skill` | Refresh existing project skill folders | GitHub → existing paths + `~/.cursor/skills/` |
| `/commit-skill` | Publish your edits | Project → GitHub + refresh `~/.cursor/` |

| | Project copy | `~/.cursor/skills/` | GitHub |
|--|--------------|---------------------|--------|
| `/configure-skills` | — | Install selected | Read |
| `/dev-pull-skill` | **Create** copy | Install | Read |
| `/pull-skill` | **Update** existing | Install | Read |
| `/commit-skill` | Read | Install published | Write |

---

## Maintaining this repo

Edit `skills/` or `commands/`, then run **`/commit-skill`** in Agent.
