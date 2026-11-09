# Public keys

Public keys that Lumen pins. They are not secret; the matching private keys live only
in the GitHub `release` environment.

| File | Key id | Used for | Decision |
| --- | --- | --- | --- |
| `lumen-code.pub` | `337EA42D297CCF5E` | Lumen's code key: Windows executables (`Lumen.exe`, `lumen-helper.exe`), checked by the privileged helper | [ADR-0063](../docs/decisions/0063-use-administrator-rights-through-an-opt-in-helper.md) rule 8 |

The updater key (`9DCAA122F70AC51F`, ADR-0048) is a different key, pinned in
`apps/desktop/src-tauri/tauri.conf.json`. Never sign one purpose with the other's key.
How to set up and rotate each key: [`docs/operations/release.md`](../docs/operations/release.md).
