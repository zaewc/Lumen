# RPM package relations

Official documentation and rpm's source checked on 2026-11-21 and 2026-11-27.
Implementation notes for
[ADR-0064](../decisions/0064-read-package-relations-from-each-managers-own-records.md).

- RPM's database format differs by distribution and release (SQLite, Berkeley DB,
  NDB), so `rpm` is asked, as for the list (#806): one `rpm -qa` with each package's
  `REQUIRENAME` and `PROVIDENAME` arrays through query-format iterators
  ([rpm-queryformat(7)](https://rpm-software-management.github.io/rpm/man/rpm-queryformat.7)),
  recorded as `RpmDatabase`. A requirement is met by a package of that name or any
  that provides it, every installed architecture included; a provider match stays
  ambiguous. `rpmlib(...)` requirements are met by `rpm` itself and left out
  ([rpm-dependencies(7)](https://rpm-software-management.github.io/rpm/man/rpm-dependencies.7)).
- "Every package implicitly provides the paths of the files it contains", and those
  do not appear in provides queries (rpm-dependencies(7)). The required files
  (at most 2,048) are asked of `rpm` once, after `--`:
  `rpm -q --whatprovides --queryformat '[%{=NAME}.%{=ARCH}\t%{FILENAMES}\n]'`. For an
  absolute path, `--whatprovides` looks in the installed file names, then in the
  provides (rpm `lib/query.cc`: `RPMQV_WHATPROVIDES` falls through to `RPMQV_PATH`,
  which looks up `RPMDBI_INSTFILENAMES`, then `RPMDBI_PROVIDENAME`). The output is
  each owner's file list; only whole lines are read, since a cut-off line could name
  a shorter path. A required file no package is found to own makes the requiring
  package's relations unknown, never "none". No file is opened; `rpm` may `lstat` an
  unowned path to word its notice.
- A boolean dependency (`(a or b)`, `(a if b)`, rpm 4.14 and later) leaves that
  package's relations unknown: its names can contain parentheses
  (`libc.so.6(GLIBC_2.34)(64bit)`) and conditions, and flattening them would lose
  their meaning or hide dependents.
- The query reads no version bounds, so a name alone never proves that a package
  meets its own requirement; it stays among the candidates.
- If the wider query does not finish within its limits (60 s, 32 MiB), the list-only
  query is asked, so the list is kept with every relation unknown. An unread package
  line or provided capability makes every relation unknown, since another provider
  could be hidden.
