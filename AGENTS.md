# AGENTS.md

## Purpose

This file is for AI agents working in this repository. Human and AI
contributors should use [CONTRIBUTING.md](CONTRIBUTING.md) as the main
contribution guide and [AI_POLICY.md](AI_POLICY.md) as the policy for
AI-assisted work.

Before starting work, read:

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [AI_POLICY.md](AI_POLICY.md)
- the related issue, including branch, scope, and acceptance criteria
- any relevant `spec/` and `plan/` documents named by the issue or affected
  code

## Instruction priority

When instructions conflict, use this priority:

1. Repository documents: [AGENTS.md](AGENTS.md),
   [CONTRIBUTING.md](CONTRIBUTING.md), [AI_POLICY.md](AI_POLICY.md), and
   `.github` templates.
2. The current issue's explicit instructions, including branch, scope, and
   acceptance criteria.
3. Relevant `spec/` documents.
4. Relevant `plan/` documents.
5. The current conversation.
6. Local implementation convenience.

If the current conversation is explicitly about changing repository guidance,
use it as the basis for the document update and make the resulting rule clear
in the changed document.

## Working rules

- If an issue gives a branch name, use that exact branch.
- If an issue does not give a branch name, use
  `issue-<issue-number>-<short-description>`.
- If scope is unclear, present a plan before editing files.
- Do not treat "handle this issue" as permission to edit files unless the issue
  and conversation make the implementation scope clear.
- Keep changes small and reviewable.
- Preserve the staged compiler pipeline described in
  [CONTRIBUTING.md](CONTRIBUTING.md).
- Follow [AI_POLICY.md](AI_POLICY.md) for pull request disclosure and commit
  trailers.

## Reporting work

When summarizing work, state:

- which issue or task was inspected
- which repository documents were checked
- what changed, if anything
- which validation commands were run
- which commands were not run and why
