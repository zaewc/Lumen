# ADR-0064: Read package relations from each manager's own records

- Status: Accepted
- Date: 2026-11-09
- Refines: [ADR-0060](0060-list-software-and-remove-it-only-through-its-own-mechanism.md)
  (rule 1, identity; rule 5, the preview names dependents and shared components),
  [ADR-0008](0008-capability-model-and-coverage.md) (unknown is never absent)
- Issues: #807 (roadmap 16.8); used by #850 (16.6), #809 (16.7), #810 (16.9)

## Context and problem statement

The software list (ADR-0060) shows what is installed and links background items to
applications by evidence (#829). It cannot yet say which package installed an
application, or what a package needs and what needs it. Three later pieces depend on
that:

- AI review's notes should state dependencies as Lumen's facts, not the model's guess
  (#850);
- a removal preview must name the dependents and shared components the mechanism also
  affects, and remove a Homebrew app through Homebrew rather than as a bare bundle
  (#809);
- a leftovers audit must tell what a package recorded from what only looks related
  (#810).

A matching file or folder name is never enough to establish ownership (#807). How
should Lumen represent these relations, and where may it read them from?

## Decision drivers

- Every relation is traceable to the record it was read from (ADR-0013's spirit).
- Recorded and inferred are told apart; only a record can verify.
- Shared items are never assigned to one package; unknown is never confirmed.
- Read only: nothing is run that could change state, and no file contents are read
  beyond the package managers' own metadata.
- Domain and application crates stay IO-free and platform-free.

## Considered options

Representation:

1. Make packages subjects of the evidence graph, with `depends_on` and `installed_by`
   edges citing evidence.
2. Record relations on the inventory's `Package`, typed and with their method, and
   resolve them against the rest of the inventory with a pure function, as background
   items are linked today (`lumen_application::software_links`).

Sources on macOS:

- Installer receipts: run `pkgutil --files` per receipt; ask `pkgutil --file-info` per
  path; or read each receipt's bill of materials (`.bom`) read-only.
- Homebrew: run `brew deps --installed` or `brew info --json=v2 --installed`; or read
  the install receipts (`INSTALL_RECEIPT.json`) Homebrew writes.

## Decision outcome

Chosen: **option 2**, read from the managers' own files, with these rules.

1. **Shape** (`lumen_domain::inventory`). `Package::relations` holds
   `PackageRelation { kind, method }`:
   - `kind` is `DependsOn { package: PackageId, direct }`, `Installed { location }`
     (an application bundle or folder, by absolute path) or
     `InstalledPackage { package: PackageId }` (a cask that ran an installer package);
   - `PackageId` is the manager and the identifier together (ADR-0060 rule 1);
   - `method` names the record (`HomebrewRuntimeDependencies`,
     `CaskUninstallArtifacts`, `CaskDefinition`, `ReceiptFileList`) or the default it
     assumed (`CaskDefaultAppFolder`), and `RelationMethod::basis` says `Recorded` or
     `Inferred`;
   - the source is the inventory report the package came in.
2. **Resolution** (`lumen_application::software_relations`, pure).
   `resolve_relations` matches each relation to a listed package by `PackageId`, or to
   a listed application whose recorded install location is the same location,
   component by component. It marks:
   - ambiguous: several matches, or packages that did not install one another each
     claiming the same application;
   - unlisted: no match, which means "not installed" only where the inventory that
     would list it was read whole (ADR-0008);
   - verified: recorded, one listed match, and unambiguous. An inferred relation is
     never verified.
3. **Ownership needs a recorded file.** A receipt owns an application only when its
   file list records the bundle's `Contents/Info.plist` as a file. Directories alone
   never count, because packages share them (`/Applications`, `/Library/Application
   Support`).
4. **Receipts: read the bill of materials** with Lumen's own bounded, read-only reader.
   A structure it does not understand makes that receipt's list unreadable and the
   source partial, never empty. Only positive matches are used.
5. **Homebrew: read the install receipts**, the same field Homebrew reads back for an
   installed keg (`Keg#runtime_dependencies` reads the tab), and for casks the
   `uninstall_artifacts` and `runtime_dependencies` it records. `brew` is not run.
6. **Relations are as of when they were recorded.** A receipt written at install time
   can name a dependency that was later removed; resolution shows it as unlisted
   rather than hiding it.
7. **Privacy.** Only package metadata is read (receipt property lists and bills of
   materials, Homebrew's JSON receipts and configuration). Relations stay on the
   device, like the list. No relation is sent to AI review until #850 decides how,
   under ADR-0061.

### Consequences

- Good: removal previews, AI notes and leftovers audits get typed, traceable facts
  with their source and strength; shared and contested items stay ambiguous by
  construction; nothing new runs.
- Good: the graph is unchanged, so evidence hashes, persistence and the policy are
  unaffected. A later producer can turn resolved relations into graph edges when a
  file decision needs them (#810).
- Bad: the bill-of-materials format is undocumented by Apple. A change would make
  receipts unreadable (reported partial), not wrong.
- Bad: Homebrew in a non-default prefix is still not found (as in #806).
- Neutral: Linux (dpkg `Depends`, file lists; RPM; pacman) and Windows (MSI
  components) relations use the same shape and come later, one source per pull
  request.

## More information

Measured on macOS 26.6.2 with Homebrew 7.0.12, 2026-11-09:

- `pkgutil --file-info` reported no package for `Contents/Info.plist` of two apps
  whose receipts list that file (`us.zoom.pkg.videomeeting`,
  `com.microsoft.Powerpoint`), so it cannot answer ownership.
- `pkgutil --files` took 5.8 s for 115 receipts (1.1 s for Xcode's 166,888 paths),
  one process per receipt. Its output is one path per line, so a file name containing
  a newline would read as two paths; a crafted name could fake a path.
- `/var/db/receipts` held 50 third-party receipts and 85 MB of bills of materials;
  Apple's own receipts live under `/Library/Apple/System/Library/Receipts` and are
  not read.
- All 216 formula install receipts carried `runtime_dependencies`, each with
  `full_name` and `declared_directly`. Casks installed by Homebrew 5 and later carry
  `.metadata/INSTALL_RECEIPT.json` with `uninstall_artifacts` (`app`, `uninstall`
  with `pkgutil` and `launchctl`, `zap`); older ones keep only the stored definition
  in `.metadata/<version>/<timestamp>/Casks/<token>.json`.
- Homebrew's sources: `cask/tab.rb` (`runtime_deps_hash`, `uninstall_artifacts`),
  `cask/artifact/relocated.rb` (an `app` target is relative to the configured
  `appdir`), `keg.rb` and `formula.rb` (installed kegs read dependencies from the
  tab).
