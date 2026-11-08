# Linux system locations

- Date: 2026-11-04
- Question: which Linux directories hold the operating system, its configuration or
  its state, so that a whole-disk scan of `/` never offers them for review? Security
  audit PLATFORM-1 ([public-beta readiness](../security/public-beta-readiness.md))
  found that the policy had no Linux system location, so `/usr/lib` could reach
  `REVIEW`.
- Outcome: `kind: system` entries for Linux in
  `crates/lumen-policy/data/protected-locations.json` (policy 0.18.0): `/usr`, `/etc`,
  `/boot`, `/efi`, `/var/lib`, `/bin`, `/sbin`, `/lib` and `/lib64`, with everything
  below them, are `KEEP` (`protect.location.linux-<name>`). Folders that hold them,
  such as `/` and `/var`, are kept by FS-3's ancestor rule
  (`protect.contains-system`). Nothing new is read: the rule compares paths only.

## Sources

Read 2026-11-04.

UAPI Group, [UAPI.9 Linux File System Hierarchy](https://uapi-group.org/specifications/specs/linux_file_system_hierarchy/),
the specification systemd's `file-hierarchy(7)` now points to:

- `/usr`: "Vendor-supplied operating system resources."
- `/etc`: "System-specific configuration."
- `/boot`: kernels and other assets used to bring up the system; `/efi`: the EFI
  System Partition when it is mounted separately.
- `/var/lib`: "Persistent system data."
- `/bin`, `/sbin`, `/lib` and, on some ABIs, `/lib64`: compatibility symlinks into
  `/usr`. On distributions without a merged `/usr` they are real directories holding
  the same kind of files, so they are listed too.

The Filesystem Hierarchy Standard 3.0 (Linux Foundation) describes the same
directories; its site refused automated reading on 2026-11-04, so it is not cited.

## Not protected, and why

- `/usr/local` is kept with the rest of `/usr`. macOS excepts it because SIP does not
  cover it; on Linux nothing documents it as removable, so the cautious choice wins.
- `/var/cache`, `/var/tmp` and `/tmp`: UAPI.9 says flushing `/var/cache` "should have
  no effect on operation of programs", and `/var/tmp` is cleaned by age. They stay
  open to the existing rules; they hold no user data the specification protects.
- `/var/log`, `/opt` and `/srv`: logs, third-party software and server data. None is
  the operating system itself; each stays at most `REVIEW`, chosen by a person.
  Roadmap 15.11 covers logs with age rules.
- `/lib32` and `/libx32`: not named by UAPI.9. Add them with a source if a
  distribution needs them.
- `/proc`, `/sys`, `/dev` and `/run`: other file systems; a scan stays on one volume
  per root, so nothing below them is listed.
