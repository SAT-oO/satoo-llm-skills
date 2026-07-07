---
name: generate-instruction-skill
description: Writes step-by-step follow-through instructions for non-technical users using a blackbox approach (actions and outcomes only, no internals). Use when the user asks for instructions, a how-to guide, a walkthrough, SOP, checklist, step-by-step directions for someone with no technical background, or translation of those instructions (especially Simplified Chinese).
---

# Generate Instruction Skill

## Goal

Produce instructions a non-technical person can follow **without understanding how anything works**. They only need to know what to do, what they should see, and what to do if something looks wrong.

Treat every system as a **blackbox**: inputs, visible results, and recovery steps — never architecture, code, or implementation detail unless the user explicitly asks for a technical version too.

## Before Writing

Gather or infer:

1. **Who** is following these steps (role, device/OS if relevant)
2. **Starting point** (logged in? app open? hardware connected?)
3. **End state** (what success looks like in plain language)
4. **Failure modes** (what might go wrong and how it looks on screen)
5. **Constraints** (time, permissions, tools they already have)

If critical details are missing, ask one short question — not a technical questionnaire.

## Writing Rules

### Blackbox only

| Do | Don't |
|----|-------|
| "Click **Save**" | "The API persists the payload to the database" |
| "Wait until you see **Connected** in green" | "The WebSocket handshake completes" |
| "If nothing happens after 30 seconds, refresh the page" | "Check the event loop / retry the RPC" |
| Name buttons, menus, and labels exactly as shown | Use internal variable or file names |

### One action per step

Each numbered step = **one** user action or **one** wait-for-result moment.

Split compound steps:
- Bad: "Open Settings, turn on Bluetooth, and scan for devices"
- Good: three separate steps

### Outcome-first steps

Format each step as: **action → expected result**.

```markdown
3. Click **Connect**.
   You should see "Device found" and a green checkmark within 10 seconds.
```

### Plain language

- Short sentences. Active voice ("Click", not "The button should be clicked").
- Define terms once in parentheses if unavoidable: "pair (link your phone to the device)".
- No acronyms without expansion on first use.
- No "simply", "just", or "obviously".

### Numbering and structure

Use this default outline:

```markdown
# [Task title — what they will accomplish]

**Time:** ~X minutes  
**You will need:** [list only what they must have in front of them]

## Before you start
- [ ] Prerequisite in plain language

## Steps
1. ...
2. ...

## You're done when
- [Observable success criterion]

## If something goes wrong
- **Symptom:** ... → **Try:** ...
```

### Screens and UI

- Refer to UI by **visible label** (button text, menu name, icon description).
- Use **bold** for exact on-screen text.
- If layout varies, describe location: "top right", "under your name".
- For hardware: describe physical parts ("the small button on the side") not model numbers unless necessary.

### Time and waiting

- Say how long to wait: "within 30 seconds", "about 2 minutes".
- Tell them what to do if the wait expires.

### Optional vs required

- Mark optional steps clearly: "(Optional) ..."
- Never hide required steps inside paragraphs.

## Translation

When instructed to translate the original instructions to another language, mostly Simplified Chinese, double check that the translation of technical terms are accurate.

### Translation rules

- Keep the same blackbox structure, step count, and outcome signals as the source.
- Leave **on-screen UI labels** in their original language (bold), with a brief gloss in the target language if helpful.
- Keep product names, file names, commands, and paths untranslated unless a well-known localized form exists.
- For technical terms (Bluetooth, UUID, protocol, API, WebSocket, etc.), use standard Simplified Chinese industry usage — not literal word-for-word guesses. When unsure, verify against official docs, the project's existing localized files, or a trusted glossary before shipping.
- Preserve numbers, timeouts, and ordering exactly.

### Language file naming

Pair each source with its translations using a consistent suffix:

| Language | Suffix example |
|----------|----------------|
| English (source) | `guide.md` or `adding-ble-protocol.md` |
| Simplified Chinese | `guide-CN.md` or `adding-ble-protocol-CN.md` |

Apply the same pattern the project already uses when one exists.

## Multi-language sync

Each time there is an agent update to either files, update all language versions at all times.

When editing any instruction file:

1. Identify all paired language versions (source + every translation).
2. Apply the same structural and factual changes to **every** version in the same session — not only the file that was opened.
3. Do not leave translations stale after adding, removing, or renumbering steps.
4. After updating, skim each version for mismatched step count, missing sections, or outdated troubleshooting.

Treat a single-language edit as incomplete until every paired file reflects the same instructions.

## Quality Checklist

Before delivering, verify:

- [ ] A non-technical reader can complete the task with only these steps
- [ ] No step assumes hidden knowledge (paths, shortcuts, jargon)
- [ ] Every step has a visible success signal or explicit "wait until"
- [ ] Failure section covers the 2–3 most likely problems
- [ ] No compound steps; numbers are sequential and complete
- [ ] Title and "You're done when" match the user's actual goal
- [ ] If translated: technical terms checked for accuracy (especially Simplified Chinese)
- [ ] If one language was edited: all paired language versions updated to match

## Output Modes

Default to **full guide** (outline above).

**Quick checklist only** — when the user wants something to print or tick off:

```markdown
- [ ] Step 1 action → expect ...
- [ ] Step 2 action → expect ...
```

**Single-block copy-paste** — when the user will send steps in chat/email: same content, no extra commentary.

**Technical appendix** — only if the user asks for both audiences: put the blackbox guide first; add a separate `## For technical readers` section at the end.

## Anti-Patterns

- Explaining *why* something works instead of *what to do*
- "Configure X" without saying where to click and what to select
- Referencing files, env vars, or CLI unless the audience is technical
- Branching logic buried in prose — use sub-steps or a small decision tree:

```markdown
4. Look at the status light:
   - **Solid blue** → go to step 5
   - **Blinking red** → see "If something goes wrong" below
```

- Steps that depend on unstated UI state ("as usual", "like before")
- Updating only the English file while leaving `-CN` or other translations behind

## Example (abbreviated)

**Task:** Connect a phone to a Bluetooth device

```markdown
# Connect your phone to the device

**Time:** ~3 minutes  
**You will need:** Phone with Bluetooth on, device charged and powered on

## Before you start
- [ ] Device is within 3 feet of your phone
- [ ] Phone Bluetooth is on (Settings → Bluetooth → On)

## Steps
1. Press and hold the power button on the device for 3 seconds.
   The small light should blink blue.
2. On your phone, open **Settings** → **Bluetooth**.
3. Under "Devices", tap the name that matches your device (e.g. "My Device").
   Wait up to 30 seconds. "Connected" should appear next to the name.

## You're done when
- Your phone shows the device as **Connected**

## If something goes wrong
- **Device not in the list** → Turn phone Bluetooth off and on, then repeat from step 1.
- **"Connection failed"** → Move the phone closer and try step 3 again.
```
