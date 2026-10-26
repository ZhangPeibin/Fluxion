# Feature Docs

Pre-PRD requirements discussions for this repo live at:

`docs/feature/<release>/<feature-slug>/discussion.md`

Formal PRDs and implementation tickets live in the issue tracker described by `docs/agents/issue-tracker.md`.

## Consumer rules

- `requirements-analysis` creates or resumes `discussion.md` at the configured path.
- The user chooses the release. Do not infer a release from the current package version unless the user confirms it.
- Derive `<feature-slug>` from the feature name as short kebab-case and state the resulting path before writing.
- Preserve prior decisions and session history when resuming an existing discussion.
- Store a structured analysis and decision record, not a verbatim conversation transcript.
- Set the discussion status to `ready-for-prd` only after the user explicitly confirms the handoff.
- Do not create `prd.md` in the feature workspace. `/to-spec` publishes the formal PRD to the configured issue tracker.

Edit the path template above if this repo adopts a different feature-document layout.
