# satoo-llm-skills

Central repository for custom Cursor skills. **GitHub is the single source of truth** — `~/.cursor/` is only an install target, never an input.

## Repository layout

```
satoo-llm-skills/
├── commands/
│   ├── commit-skill.md      # Push workspace skills → GitHub
│   └── pull-skill.md        # Pull GitHub skills → external project
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

## Pulling a skill into your project

If your project already has a `*-skill` folder and you want the latest version from this repo:

1. Run `/pull-skill` in Cursor Agent from that external project.

The command will:
1. Clone this repo from GitHub
2. Match workspace `*-skill` folders by name against `skills/` in the central repo
3. Overwrite the project copy with the remote version
4. Update `~/.cursor/skills/` for the pulled skill(s)

This is the reverse of `/commit-skill` — use pull to consume updates, commit to publish changes.

## Maintaining this repo

Edit skills under `skills/` or update `commands/commit-skill.md` directly in this repository, then run `/commit-skill`. The command publishes those changes to GitHub and reinstalls the updated command definition globally — no dependency on `~/.cursor/` as a source.
