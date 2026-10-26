# Worktrees

Implementation tasks use isolated Git worktrees under:

`.worktrees/<task-slug>`

The worktree directory is relative to the repository root. The root `.gitignore` must contain `/.worktrees/`.

## Consumer rules

- Create a worktree only for a new implementation task. Reading, discussion, and review do not need one.
- If already inside a linked worktree, continue there. If the current checkout already has uncommitted changes for the task, continue there rather than moving them.
- Prefer the ticket or feature slug for `<task-slug>`; otherwise derive short kebab-case from the task and state the path before creating it.
- Create or reuse the task branch when setting up the worktree, following the repository's existing branch conventions. This policy does not define branch prefixes.
- Before reusing an existing path, verify it with `git worktree list`. Reuse it only when it belongs to the same task; otherwise stop and ask the user. Never overwrite, remove, or silently rename it.
- Remove a finished worktree only when it is clean and its work is safely retained. Preserve dirty worktrees and tell the user.
- Do not migrate or remove worktrees created under an older layout unless the user explicitly requests it.
