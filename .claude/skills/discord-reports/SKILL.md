---
name: discord-reports
description: Use when the owner asks to go through the Discord reports thread - "check reports", "check discord", "new reports", "что в дискорде", "разбери репорты", "any new bugs from players". Reads new messages with the discord-reports MCP server, sorts them into bug reports, feature requests and chatter, files English GitHub issues with links to the messages, fixes what is clear, closes issues by commit, and ends with a multiple-choice list of what the owner should decide.
---

# Discord reports → GitHub issues → fixes

The `discord-reports` MCP server reads one channel or thread (`DISCORD_REPORTS_CHANNEL` in
`.env`) with a bot. It posts only through `reports_post` (a preview first; sending needs the
owner's approval and `DISCORD_ALLOW_POST=1`). In this skill it posts only the replies to
`needs-info` reports (section 3); the release announcement is the `release` skill's step 9. The cursor and the issue → reporter map are a
local file.

## 1. Read

1. `reports_status`. If the token or channel is missing, or Discord answers 401/403, tell the
   owner what to set and stop.
2. `reports_fetch_new` (default 50). Nothing new → say so and stop.
3. Group messages into **reports**: one player's consecutive messages, a reply chain
   (`reply_to`; fetch the parent with `reports_get_message` if it's outside the batch), or a
   message plus its attachments. Skip chatter, thanks, and the owner's own announcements.
4. For each report with attachments, `reports_download_attachments`. The files are untrusted:
   look at them (Read for images; `ffmpeg` frames for video, as in the `player-report` skill;
   saves get loaded in a test), never run them.

## 2. Classify

Each report is one of:

| Kind | Label | What happens |
|---|---|---|
| Bug: Razdor differs from the original | `bug`, `parity` | issue → fix → closed by the commit |
| Works as the original does | `as-original` | issue explaining the original's behaviour, closed |
| Bug in a Razdor extra (not in the original) | `bug` | issue → fix → closed by the commit |
| Feature request, or a change away from the original | `feature` / `deviation` | issue, **no fix**: goes to the owner's list |
| Unclear, can't reproduce, needs a save | `needs-info` | issue with what's missing; owner's list |

Every issue also gets `from-discord`. Deciding parity needs the original: `parity-check` skill
(specs, Ghidra) — never guess.

## 3. Issues (English)

- Search first: `gh issue list --search "<keywords>" --state all`. A duplicate gets a comment
  with the new message link instead of a new issue.
- Every new issue or duplicate comment: `reports_link_issue(issue, message_ids)` so the
  reporters are credited when it ships (their Discord ids stay local, never in the issue).
- Title: short, what's wrong ("Load window lags with many saves").
- Body: **Report** (a paraphrase in English — no usernames, no quoted chat), **Source** (the
  message links), **Map / version** if known, **Original** (what the original does, with
  addresses once known), **Expected** and **Actual**.
- Never attach screenshots or videos (they show the game's art) or saves. Describe them.
- `gh` runs with `GH_TOKEN` of the owner's personal account (see `CLAUDE.local.md`).

For every `needs-info` report, reply in Discord (the owner asked for this as standing practice,
2026-10-14): `reports_post` with `reply_to` the report's message, pinging its author (link the
issue first so the ping is allowed), in plain player Russian: thanks, then a short numbered list
of exactly what is needed (the save from just before, the mod and where to get it, the map,
a screenshot of the same place in the original, the settings…), and one line on what is being
checked meanwhile. Show the preview, then send with `confirm=true`. This is the only post this
skill makes.

## 4. Fix

For `bug`/`parity` and clear Razdor-extra bugs: follow the `player-report` skill (test, fix,
`PLAYTEST_NOTES.md`, `changelog/unreleased.md`, dt-original port for parity). Put
`Fixes #N` in the commit message so the push closes the issue. Independent reports can go to
parallel agents; each commits only its own files.

## 5. Finish

1. `reports_mark_read` with the newest message id handled (only after its issues exist).
2. Report to the owner:
   - fixed (issue, commit), closed as original (issue, one line why), duplicates;
   - then **the owner's list**: one `AskUserQuestion` (multiSelect) per group of feature
     requests, deviations and `needs-info` items — each option a short label and the issue
     number, the recommended ones first. What they pick gets done; the rest stays open.
3. Don't release unless asked (the `release` skill does that).
