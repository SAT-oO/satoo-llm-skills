# satoo-llm-skills

Central repository for custom Cursor skills. **GitHub is the single source of truth** — `~/.cursor/` is only an install target, never an input.

## Repository layout

```
satoo-llm-skills/
├── commands/
│   └── commit-skill.md      # Self-updating slash command (canonical definition lives here)
├── bootstrap.sh               # One-time setup: installs from GitHub → ~/.cursor/
└── skills/                    # Skill packages (each folder ends with -skill)
    └── ble-hack-skill/
        └── SKILL.md
```

## For any developer — first-time setup

Install commands and skills from GitHub (nothing is read from local copies):

```bash
curl -sSL https://raw.githubusercontent.com/SAT-oO/satoo-llm-skills/main/bootstrap.sh | bash
```

This installs:
- `commands/` → `~/.cursor/commands/`
- `skills/` → `~/.cursor/skills/`

Re-run bootstrap any time you want to pull the latest from GitHub without publishing changes.

## Publishing a skill from your project

1. In your project, create a folder named `*-skill` with a `SKILL.md` file (e.g. `my-database-skill/SKILL.md`).
2. Run `/commit-skill` in Cursor Agent.

The command will:
1. Clone this repo from GitHub
2. Merge your workspace skill(s) into `skills/`
3. Push to `main` if there are changes
4. Reinstall `commands/` and `skills/` from the repo into `~/.cursor/`

## Maintaining this repo

Edit skills under `skills/` or update `commands/commit-skill.md` directly in this repository, then run `/commit-skill`. The command publishes those changes to GitHub and reinstalls the updated command definition globally — no dependency on `~/.cursor/` as a source.
