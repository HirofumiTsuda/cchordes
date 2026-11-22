---
name: create-issue
description: Create a GitHub issue in this repo with the right labels, and check that the labels were really applied. Use whenever the user asks to open, file or "cut" an issue. Writes the title and body in English, even if the conversation is in Japanese.
---

# Create issue

Create an issue in this repo, label it, and read the labels back.

## Steps

1. **Check it is not a duplicate.**
   `gh issue list --state all --search "<keywords>"`. If one exists, comment
   on it or reopen it instead.

2. **Write the title and body in English**, whatever language the
   conversation is in. Shape:
   - A problem or feature: `## Problem`, `## Proposal`, then `## Not covered`
     and `## Done when` when they help. State what was measured and what was
     not. Link related issues or PRs.
   - A bug: `## What happened`, `## Expected`, `## Steps to reproduce`, and
     the environment (OS, terminal, cchordes commit).
   - Text posted under the maintainer's name: short plain sentences, no em
     dashes.

3. **Pick the labels** from this repo's set (check with `gh label list`).
   One for the kind, then any others that apply:

   | Label | Use for |
   |---|---|
   | `bug` | Something does not work |
   | `enhancement` | A new feature or capability |
   | `documentation` | Docs only |
   | `question` | Further information is requested |
   | `accessibility` | Barrier affecting people with disabilities |
   | `good first issue`, `help wanted` | Add only if a contributor is wanted |

   Do not invent labels. If a missing one is clearly needed, ask the user
   before creating it.

4. **Create it with `gh api`, and send the labels as JSON.** Do not use
   `-f 'labels[]=...'`: with some `gh` versions the issue is created with no
   labels, and nothing says so.

   ```bash
   gh api -X POST repos/HirofumiTsuda/cchordes/issues --input - <<'EOF'
   {"title": "...", "labels": ["enhancement"], "body": "..."}
   EOF
   ```

   Build the JSON with `python3 -c 'import json; ...'` when the body has
   quotes or backticks, instead of escaping by hand.

5. **Read the labels back before saying they are set.**

   ```bash
   gh issue view <number> --json labels -q '[.labels[].name]|join(",")'
   ```

   If the list is empty or short, add them with
   `gh api -X POST repos/HirofumiTsuda/cchordes/issues/<number>/labels --input -`
   and `{"labels": [...]}`, then read again. Report the labels that are
   actually on the issue, not the ones that were asked for.

6. **Give the user the issue URL** and the labels that are on it.
