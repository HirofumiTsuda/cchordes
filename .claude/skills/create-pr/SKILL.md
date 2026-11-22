---
name: create-pr
description: Create (or update) a GitHub pull request for the current branch in this repo. Use whenever the user asks to open, create, or update a PR. Writes the title/body in English with What / Why / QA / Ref sections, even if the conversation is in Japanese.
---

# Create PR

Create a pull request for the current branch against `main`.

## Steps

1. **Name the branch `<type>/<short-slug>`** when creating a new branch for
   the work (before starting, not at PR time). `<type>` mirrors Conventional
   Commit types — `feat`, `fix`, `docs`, `refactor`, `chore`, `test`, `ci`,
   `perf`, `build` — picked to match the nature of the change. `<slug>` is a
   short kebab-case description (e.g. `ci/rust-tests`,
   `feat/pane-scrollback`). Do **not** put issue numbers in the branch name —
   link issues from the PR body's `Ref` section instead.

2. **Check state.** Run `git status`, `git log main..HEAD`, and
   `git diff main...HEAD` to see what's actually being shipped. If there are
   uncommitted changes relevant to the PR, ask the user before including
   them — don't silently commit unrelated work. Keep each PR to one
   independent concern; split unrelated changes into separate PRs.

3. **Push the branch** if it isn't already tracked upstream:
   `git push -u origin <branch>`.

4. **Write the body** with these sections, based on the actual diff and
   commits — don't invent content that isn't supported by the change. If
   `.github/PULL_REQUEST_TEMPLATE.md` exists, use it instead.

   - `What` — what changed
   - `Why` — motivation
   - `QA` — how it was verified (e.g. `cargo test`, manual run); say plainly
     if something was not verified
   - `Ref` — related issues/PRs (e.g. `Closes #26`)

   End the body with the attribution line given in the session's
   system-reminder, if any.

5. **Language: always write the PR title and body in English**, regardless
   of the conversation language. Applies to new PRs and edits.

6. **Create or update the PR using `gh api`, not `gh pr create --body` /
   `gh pr edit --body`.** `gh pr create`/`gh pr edit` can intermittently
   fail with `GraphQL: Projects (classic) is being deprecated ...
   (repository.pullRequest.projectCards)`. The error comes from the CLI's
   follow-up query, and the create/update may have partially succeeded.
   Call the REST API directly:

   - New PR:
     ```bash
     gh api -X POST repos/{owner}/{repo}/pulls \
       -f title="..." \
       -f head="<branch>" \
       -f base="main" \
       -f body="$(cat <<'EOF'
     ...body text...
     EOF
     )"
     ```
   - Update an existing PR:
     ```bash
     gh api -X PATCH repos/{owner}/{repo}/pulls/{number} \
       -f body="$(cat <<'EOF'
     ...body text...
     EOF
     )"
     ```

   Get `{owner}/{repo}` from `git remote get-url origin` if not already
   known.

7. **Verify** by reading back the result (`gh pr view <number> --json
   url,title,body`, or the API response) and report the PR URL to the user.
