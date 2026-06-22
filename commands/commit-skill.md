# commit-skill

When this command is invoked, pull the latest state from the remote repository, merge any local agent skill directories discovered inside the active workspace root, back up this command file, and push the combined state back to GitHub. Execute the entire lifecycle via the terminal tool without user intervention.

### Target Destination
* **Repository URL:** `https://github.com/SAT-oO/satoo-llm-skills.git`
* **Target Branch:** `main`

### Execution Protocol
1. **Locate Active Workspace:** Resolve the absolute path of the current project workspace.
2. **Scan for Workspace Skills:** Identify all folders matching the pattern `*-skill` inside the active workspace root.
3. **Setup Isolation Workspace:** Provision an isolated temporary synchronization directory: `/tmp/satoo-skills-sync-$(date +%s)`. Clone the destination repository directly into this directory to establish the remote repo as the source of truth.
4. **Consolidate Assets & Self-Manage:**
    * Create a `commands/` folder inside the cloned temporary repository if it does not exist.
    * Copy the active global command file itself from `~/.cursor/commands/commit-skill.md` into the temporary repository's `commands/` directory.
    * Execute `rsync -a` (without `--delete`) to copy all detected workspace `*-skill` folders into the `skills/` directory of the temporary repository. This adds or updates skills without wiping out existing skills stored remotely.
5. **Evaluate Delta & Publish:** Run `git status --porcelain` inside the temporary repository.
    * If structural changes exist, stage all files (`git add -A`), commit with a precise timestamped metadata message (`Automated sync: YYYY-MM-DD HH:MM:SS (remote-first aggregation)`), and push to `origin main`.
    * If no changes are detected, skip the push lifecycle.
6. **Update Global Storage:**
    * Mirror the complete, consolidated state of the skills and commands from the temporary repository back into your global home directory path at `~/.cursor/` using `rsync -a --delete`.
    * Force a system re-index of the Cursor environment rules engine so changes register globally across all open editor instances.
7. **Purge Workspace:** Recursively remove the temporary clone folder from the filesystem (`rm -rf`).

### System Response Format
Provide a real-time status log in the chat window using the exact schema below:
* `[+] Resolving active workspace paths...`
* `[+] Fetching remote source of truth from satoo-llm-skills...`
* `[+] Aggregating local project workspace skills into repository tree...`
* `[+] Staging global commit-skill command for self-management...`
* `[+] Executing Git push to satoo-llm-skills...`
* `[+] Syncing finalized repository state down to global storage at ~/.cursor/...`
* `[+] Workspace cleanup finalized.`

This command will be available in chat with /commit-skill