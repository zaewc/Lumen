# CLAUDE.md

@AGENTS.md

## Claude Code specifics

- Use targeted search (Grep/Glob) and read only the files a task needs; the ADR index
  in `docs/decisions/README.md` is the fastest way to find prior decisions.
- For web lookups, prefer official documentation (Apple Developer, Microsoft Learn,
  Android Developers, docs.rs, Tauri, Expo, GitHub Docs) and record findings that
  change a decision.
- Long tasks: work through roadmap items one PR at a time; report which PRs merged and
  what remains, including any check that failed or was skipped.
- Never add `Co-Authored-By` or other attribution trailers to commits or PRs.
