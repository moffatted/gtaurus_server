---
applyTo: "**/*.md"
description: Use when creating or editing Markdown files to enforce markdownlint-cli cleanup before finalizing.
---

# Markdown Lint Enforcement

## Rule

Always run Markdown lint CLI after creating or editing any `.md` file and fix all warnings in files touched by the task.

## Required Command

Run this from the repository root:

```bash
npx -y markdownlint-cli2 "**/*.md"
```

## If Warnings Are Reported

- Fix the reported issues directly in the affected markdown files.
- Re-run the lint command until the touched files are clean.
- Do not ignore warnings unless the user explicitly asks to keep a specific violation.

## Scope

- This rule applies to docs, rules, prompts, skills, workflows, and any other markdown content.
- Prefer minimal edits focused on lint fixes unless broader cleanup is requested.
