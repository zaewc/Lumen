# dpkg package relations

Official documentation checked on 2026-11-21. Implementation notes for
[ADR-0064](../decisions/0064-read-package-relations-from-each-managers-own-records.md),
moved out of the accepted ADR (ADR-0001: accepted ADRs change only in status and
links).

## A need any one of several packages meets

Linux records state needs that one `DependsOn { package }` cannot express without
guessing: dpkg's alternatives (`a | b`), and a name that other packages provide (a
virtual package). Such a need is `PackageRelationKind::DependsOnOneOf { packages,
direct }` and is never narrowed to one of them (#970). In
`lumen_application::software_relations::resolve_relations`, every candidate of a
`DependsOnOneOf` is ambiguous, even a lone listed one, so none is verified, and in
removal facts each candidate's dependents count is a lower bound. pacman (research
39) and RPM (research 40) use the same kind.

## dpkg's status file

- `/var/lib/dpkg/status` is read whole (as for the list, #806). A present package's
  `Pre-Depends` and `Depends` are its needs (`DpkgStatus`); `Recommends` and
  `Suggests` are not needs to run. A need for a name is met by the package of that
  name or by any package that `Provides` it, and alternatives (`a | b`) by any one
  ([Debian Policy 7.1 and 7.5](https://www.debian.org/doc/debian-policy/ch-relationships.html)).
  Only a single name that exactly one installed package has, and none provides,
  becomes `DependsOn`; any other need is `DependsOnOneOf` with every candidate.
- An architecture qualifier (`:any`, `:amd64`;
  [deb-control(5)](https://manpages.debian.org/unstable/dpkg-dev/deb-control.5.en.html))
  is dropped, and every installed architecture of the name is a candidate: Multi-Arch
  rules would narrow it, but a wrong narrowing would hide the package that meets the
  need, while a wider set only makes it ambiguous.
- Relationship fields are folded only in source control files (Debian Policy 7.1),
  but continuation lines are joined anyway, since the status file is untrusted text.
- A relation that is not `name[:arch] [(op version)]` with a name Debian Policy
  5.6.1 allows makes that package's relations unknown. A version clause must use a
  Debian relationship operator and valid version syntax (Debian Policy 5.6.12);
  `Provides` allows only `=`. A paragraph without a name or status, or a `Provides`
  that cannot be read, makes every package's relations unknown, since a need's
  candidates are then not known whole.
- Versions are not evaluated: if any need is versioned, a package's own name or
  provided name does not prove it meets its own need, so every candidate is kept.
  This keeps external providers visible when an unversioned self-provide cannot meet
  a versioned need (Debian Policy 7.5).
- `/var/lib/dpkg/info/<package>.list` (the files a package installed) is not read:
  no Linux inventory lists applications by install location for it to link to, and
  per-package file lists for removal previews are #809.
