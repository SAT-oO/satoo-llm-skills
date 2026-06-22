# satoo-llm-skills

Collection of custom LLM skills to automate workflows of past projects. Synced from the `/commit-skill` Cursor command.

## Repository layout

```
satoo-llm-skills/
├── commands/
│   └── commit-skill.md      # Cursor slash command
├── bootstrap.sh             # Installs skills + commands to ~/.cursor/
└── skills/                  # Skill folders (each ends with -skill)
    └── ble-hack-skill/
        └── SKILL.md
```

## Setup

At project initialization, run the bootstrap script to install skills and commands globally:

```bash
curl -sSL https://raw.githubusercontent.com/SAT-oO/satoo-llm-skills/main/bootstrap.sh | bash
```

This mirrors `commands/` → `~/.cursor/commands/` and `skills/` → `~/.cursor/skills/`.

## Adding or updating a skill

1. Create or edit a skill folder whose name ends with `-skill`, containing a `SKILL.md` file.
2. Run `/commit-skill` in Cursor Agent. It will:
   - Update this repo with the latest skill content from your workspace
   - Refresh the Cursor skill files in `~/.cursor/skills/`
