# Native cleanup scope and application prerequisites

- Date: 2026-10-30
- Question: which researched cleanup changes can recover more storage with the
  current execution safeguards?
- Outcome: widen Homebrew's confirmed native cache cleanup; defer Adobe cache
  quarantine until application-exit prerequisites can be enforced.

## Homebrew downloads

The [official cleanup reference](https://docs.brew.sh/Manpage#cleanup-options-formulacask-)
says the default command removes outdated downloads and downloads older than 120
days. `--prune=all` removes that age threshold; `--scrub` includes downloads for
current versions. The [implementation](https://github.com/Homebrew/brew/blob/main/Library/Homebrew/cleanup.rb)
checks age pruning before its installed-version-aware stale check, so `--prune=all`
can remove downloads for installed packages too. Do not apply the plain `--scrub`
documentation's installed-download exception to this combined command.
The command also retains its normal cleanup of stale locks and old installed versions.

Use `brew cleanup --prune=all --scrub`, with the same options plus `--dry-run` for
preview. Keep the entry `tool_action`: the exact command is shown, explicitly
confirmed, and run by the reviewed host runner without a shell. Explain that a future
installation may need to download files again. No direct cache removal, permission
escalation, package uninstall, or additional force flag is introduced.

Native tools decide their own eligibility. The cache's displayed size is not a promise
that the command removes all of it; observed volume changes remain separate from the
estimate. Verification executes an argument-printing fixture program, never real
Homebrew cleanup.

## Adobe media cache: excluded for now

Adobe [documents regenerable media caches](https://helpx.adobe.com/premiere/desktop/troubleshooting/media-issues/manage-media-cache.html),
but its [manual cleanup procedure](https://helpx.adobe.com/premiere/desktop/troubleshooting/media-issues/delete-media-cache-files-manually.html)
requires Premiere to be closed. Lumen's file-handle checks establish whether observed
files are open, not whether all relevant Adobe applications have exited. They do not
prove that prerequisite. Do not add a quarantinable Adobe entry on the assumption
that an idle application has no open cache handles. A future integration needs an
enforced exit check or a supported vendor cleanup action; original media, projects,
autosaves and the shared cache database remain outside this proposed scope.
