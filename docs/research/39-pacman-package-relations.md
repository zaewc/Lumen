# pacman package relations

Official format documentation checked on 2026-11-24. Implementation notes for
[ADR-0064](../decisions/0064-read-package-relations-from-each-managers-own-records.md).

Linux, pacman (2026-11-21):

- Each `/var/lib/pacman/local/<package>/desc` is read, as for the list. Its
  `%DEPENDS%` are its needs (`PacmanLocalDatabase`); `%OPTDEPENDS%` are not needs to
  run. A value is `name` or `name<op>version`
  ([alpm-db-desc(5)](https://man.archlinux.org/man/alpm-db-desc.5),
  [alpm-package-relation(7)](https://man.archlinux.org/man/alpm-package-relation.7.en),
  [alpm-comparison(7)](https://man.archlinux.org/man/alpm-comparison.7.en)), with a
  name [alpm-package-name(7)](https://man.archlinux.org/man/alpm-package-name.7.en)
  allows; a soname relation (`lib:libfoo.so.1`) is matched as written. pacman has no
  alternatives, so a need is ambiguous only where another package's `%PROVIDES%`
  names it, and the rules are dpkg's: a `desc` without a name or a `%PROVIDES%` that
  cannot be read makes every relation unknown, a `%DEPENDS%` that cannot be read only
  that package's.
- The `files` list is not read, for the reasons given for dpkg's `.list` files.
- Comparisons must have an allowed operator and a nonempty version; provisions
  allow only `=`. As with dpkg, versioned needs retain self and external candidates
  because name matching alone cannot prove that the version is met.

A section ends at the next header even without an empty line. A missing name
therefore makes the record unread, rather than naming the following header as a
package. An unread directory entry makes the inventory partial and all dependency
relations unknown because another provider may be hidden.

A section that is present with no value (pacman omits a section with no data) is
unread, never empty: an empty `%DEPENDS%` leaves that package's relations unknown,
an empty `%PROVIDES%` every package's.
