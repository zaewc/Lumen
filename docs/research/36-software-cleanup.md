# 36. Software inspection and removal: Hoax Eliminator, Universal Windows Debloater and native mechanisms

- Date: 2026-11-07
- Issue: #805 (part of #804)
- Method: public sources and official platform documentation only. No copy of Hoax
  Eliminator was downloaded, run or disassembled, and no Windows test machine was
  available, so every statement about its internals is marked as such. Facts that
  official documentation did not confirm are marked **unconfirmed**.

## Summary

- Hoax Eliminator (구라제거기) is a closed-source Windows tool that finds a built-in
  list of Korean banking and government "security" modules among installed programs
  and runs each one's own uninstaller from a generated batch file. Its useful ideas are
  a curated, cited target list, removal through the vendor's uninstaller, a "keep"
  list, a rescan after removal, and honest advice that sites reinstall what they need.
  Its weak points, by Lumen's standards, are a batch file, administrator rights from
  the start, no preview of what each uninstaller will do, and no verification beyond a
  rescan.
- Every platform has an inventory Lumen can read without changing anything, and a
  removal mechanism it can invoke. None of them can be undone in general, so software
  removal cannot reuse quarantine's "reversible until finalized" promise.
- Lumen already reads most of the inventory (macOS bundles and launchd plists; Windows
  Uninstall keys, Run keys, Startup folder and services) but uses it only to judge
  files. Nothing lists software, reads uninstall metadata, links services to
  applications, or covers Linux.
- Recommendation: build #806 as a read-only inventory first and show it. Removal
  (#809) should follow the ADR-0049 pattern: the command comes from the platform's own
  record for one exact software identity, never from a page, the user or AI. It runs
  without a shell after the user confirms that exact target, and success is claimed
  only when a fresh inventory no longer lists it. A proposed ADR (ADR-0060, its own
  pull request) records this for the owner's decision.

## Hoax Eliminator

Sources: the author's page <https://teus.me/HoaxEliminator> and the 7.62 notes
<https://teus.me/hoaxeliminator/HoaxEliminator7.55/>; a 2018 interview
<https://zdnet.co.kr/view/?no=20180605195522>; a third-party guide
<https://dnublog.co.kr/2026/04/16/구라제거기-다운로드-사용법-비밀번호·실행방법·/>.

| Question | Finding | Status |
| --- | --- | --- |
| Author, version | "BLUEnLIVE" (teus.me); 7.62, 2026-10-21; x86, x64, ARM64 builds in a zip | verified |
| License, terms | none published; bundles open-source libraries (Abseil, Zstd) since 7.56 | **unknown** |
| Source | closed | verified (interview) |
| Discovery | finds targets "in the installed-programs list" | verified (author) |
| Which list | presumably the Windows Uninstall keys; services, files, processes not stated | **unverified** |
| Target list | built into each release; changelog mentions a merged "regex list" and added targets | verified (author) |
| Server updates | none described | **unknown** |
| Removal | generates a batch file that runs each selected program's uninstaller in turn; dialogs may appear | verified (author) |
| Silent switches, direct deletion, force mode | not stated | **unknown** |
| Privileges | needs administrator rights | verified (third party) |
| After removal | "reload list" rescans; some items need several passes; restart advised | verified (third party) |
| Restore points, backups, undo | none described | **unknown** (probably none) |
| User controls | checkbox list, remove selected or all, a "work program" list kept from removal | verified (third party) |
| Platforms | Windows only; no macOS or Linux equivalent found | verified |

Caveats reported by users: banking and government sites reinstall their modules on the
next visit, and work PCs can lose company security software. Neither the target list
nor anything else from the program may be copied: it is not licensed, and Lumen needs
its own cited sources anyway.

What Lumen takes as product concepts (not implementation): a curated list of known
software with a reason for each entry; removal through the software's own uninstaller;
an explicit keep list; a fresh inventory after removal; plain warnings about what
returns or breaks. What Lumen does differently: no shell or batch file; no blanket
administrator rights; one exact target per confirmation; a preview of the mechanism
and its effects; verification instead of a claim; and evidence for every statement,
including "security software" status, which is protected rather than targeted.

## Universal Windows Debloater

Added 2026-11-07 at the owner's request as a second reference. Sources: the project page
<https://samueljayasingh.github.io/Universal-Windows-Debloater/> (last edited
2023-06-27) and its repository
<https://github.com/samueljayasingh/Universal-Windows-Debloater> (MIT, last push
2023-06-26). The script was read as text from the repository; it was not run.

| Question | Finding | Status |
| --- | --- | --- |
| What | A PowerShell script for Windows 10 and 11: removes preinstalled apps, disables telemetry, Bing search and Cortana, and hides interface suggestions and Explorer entries | verified (page) |
| How it runs | `Run.bat` (elevates through UAC) with a default or custom menu, or the `.ps1` as administrator after `Set-ExecutionPolicy Unrestricted -Force`; switches such as `-RemoveApps` for unattended use | verified (page) |
| What it removes | Apps named in a list inside the script (`Microsoft.BingWeather`, `Microsoft.Todos`, `Spotify`, `king.com.CandyCrushSaga`, …); another list is kept (`Microsoft.WindowsCalculator`, `Microsoft.Windows.Photos`, `Microsoft.WindowsStore`, …) | verified (page, script) |
| Removal mechanism | `Get-AppxPackage -Name $app -AllUsers \| Remove-AppxPackage`, then `Remove-ProvisionedAppxPackage -Online -AllUsers` for provisioned packages that match `-like $app`: every user's copy and the copy new accounts would get | verified (script lines 111 and 113) |
| Settings changes | `reg import` of `.reg` files from its `Regfiles` folder; matching files revert them | verified (script line 126, page) |
| Restore point or backup | none in the script | verified (script) |
| Undo for apps | reinstall from the Microsoft Store; the page says the Store itself and the Xbox speech-to-text overlay cannot be reinstalled that way | verified (page) |
| Services, scheduled tasks, Edge, Defender | not mentioned on the page | **unknown** |
| License | MIT | verified (repository) |

What Lumen takes as product concepts:

- a curated list of preinstalled consumer apps, each with a stated reason, beside an
  explicit keep list (the Store, Calculator, Photos);
- a default selection the person can change one item at a time;
- for each app, telling the person where it can be reinstalled from (here, the Microsoft
  Store) and which apps cannot be reinstalled at all.

What Lumen does not do, by its rules:

- **Every user and new accounts.** Removing for all users and deleting provisioned
  packages needs administrator rights and affects other people's accounts. ADR-0060
  (owner decision) runs only what the user's own rights allow: MSIX removal for the
  current user.
- **Name patterns.** Wildcard name matching (`-like`) can hit packages that only look
  alike. Lumen removes by exact package family name from the inventory.
- **System settings.** Telemetry, search, Explorer and context-menu changes alter how
  Windows behaves, not what is stored. They are outside a storage app's purpose and
  are not reversible by Lumen's quarantine.
- **Execution policy.** Lumen never asks anyone to lower PowerShell's execution policy
  and runs no script.

Although the MIT license would allow reusing the script's app lists with attribution,
Lumen's list needs a cited reason per entry (ADR-0021, ADR-0060). The script's names
are at most a starting point to research, not entries to copy.

## Discovery by platform

| Platform | Source | Identity | Lumen today |
| --- | --- | --- | --- |
| macOS | `.app` bundles in `/Applications`, `~/Applications` | `CFBundleIdentifier` (unconfirmed page body) | `AppInventory`, one level deep |
| macOS | package receipts: `pkgutil --pkgs`, `--files`, `--pkg-info` ([man](https://ss64.com/mac/pkgutil.html)) | package ID | not read |
| macOS | Homebrew formulae and casks ([Cask Cookbook](https://docs.brew.sh/Cask-Cookbook)) | token | not read |
| macOS | LaunchAgents and LaunchDaemons in five directories ([launchd](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html)) | label | `LaunchdInventory` (plists only) |
| macOS | Login items and background tasks (macOS 13+, [Apple](https://support.apple.com/guide/deployment/managing-login-items-background-tasks-mac-depdca572563/web)) | bundle ID, team ID, label | not read; `sfltool dumpbtm` without root **unconfirmed** |
| Windows | Uninstall keys, HKLM 64/32-bit and HKCU ([MSI](https://learn.microsoft.com/en-us/windows/win32/msi/uninstall-registry-key)) | key name; product code for MSI | `InstalledApps` (name, version, publisher, location only) |
| Windows | MSI products via `MsiEnumProductsEx` ([API](https://learn.microsoft.com/en-us/windows/win32/api/msi/nf-msi-msienumproductsexw)) | product code | not read |
| Windows | MSIX/AppX packages (`Get-AppxPackage`, [docs](https://learn.microsoft.com/en-us/powershell/module/appx/get-appxpackage)) | package family name | not read (roadmap 6.10) |
| Windows | services via the service manager ([docs](https://learn.microsoft.com/en-us/windows/win32/services/service-configuration-programs)) | service name | `Services` |
| Windows | Run/RunOnce keys ([docs](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys)), Startup folder | value name | `StartupEntries`; Task Scheduler and `StartupApproved` not read (**unconfirmed** docs) |
| Linux | dpkg/apt (`apt-mark showmanual`, [man](https://manpages.debian.org/trixie/apt/apt-mark.8.en.html)) | package name | none |
| Linux | rpm/dnf (`dnf repoquery --userinstalled`, [docs](https://dnf.readthedocs.io/en/latest/command_ref.html)) | package name | none |
| Linux | pacman (`pacman -Qe`, [man](https://man.archlinux.org/man/pacman.8)) | package name | none |
| Linux | Flatpak (`flatpak list --app`, user and system, [docs](https://docs.flatpak.org/en/latest/flatpak-command-reference.html)) | application ID + installation | none |
| Linux | Snap (`snap list`) | snap name | none (**unconfirmed** docs) |
| Linux | systemd units (`systemctl list-unit-files`, `--user`, [man](https://www.freedesktop.org/software/systemd/man/latest/systemctl.html)), XDG autostart ([spec](http://specifications.freedesktop.org/autostart/latest/)) | unit or file name | none |

Do not use WMI `Win32_Product`: querying it makes Windows Installer verify and repair
every product ([Microsoft](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/msiprov/win32-product)).
Hidden entries are real: `ARPSYSTEMCOMPONENT` hides an MSI product from Settings while
it stays installed ([Microsoft](https://learn.microsoft.com/en-us/windows/win32/msi/configuring-add-remove-programs-with-windows-installer)).

## Removal by platform

| Mechanism | How | Non-interactive | Reports failure | Notes |
| --- | --- | --- | --- | --- |
| macOS vendor uninstaller | run the uninstaller the vendor ships | varies | varies | Apple: use it if present, otherwise move the app to the Trash ([Apple](https://support.apple.com/guide/mac-help/install-and-uninstall-other-apps-mh35835/mac)) |
| macOS move to Trash | `.app` bundle to the Trash | yes | the move does | leaves data in `~/Library`; apps macOS requires cannot be removed; SIP protects system apps ([Apple](https://support.apple.com/en-us/102149)) |
| macOS `pkgutil --forget` | forgets the receipt | yes | yes | **removes no files**; never a removal |
| Homebrew `brew uninstall` | cask `uninstall` stanza: quit, launchctl, login_item, pkgutil, script, delete… | yes | exit status | a cask may run a script with `sudo`; `--zap` also removes `~/Library` data and is never the default ([Cookbook](https://docs.brew.sh/Cask-Cookbook)) |
| Windows MSI | `msiexec /x {ProductCode}` | `/qn`, log with `/L*v` | exit codes: 0 ok, 3010 and 1641 ok with reboot, 1602 cancelled, 1603 failed, 1618 busy ([codes](https://learn.microsoft.com/en-us/windows/win32/msi/error-codes)) | product code from the inventory, not a string |
| Windows registered uninstaller | `UninstallString` (or `QuietUninstallString`) | sometimes | the uninstaller's own | an arbitrary command line written by the installer: **untrusted** |
| Windows MSIX | `Remove-AppxPackage` / `PackageManager` | yes | yes | current user only without admin; provisioned packages need `Remove-AppxProvisionedPackage` too ([docs](https://learn.microsoft.com/en-us/powershell/module/appx/remove-appxpackage)) |
| Windows winget | `winget uninstall` | `--silent`, `--disable-interactivity` | **unconfirmed** exit codes | can act on software it did not install ([docs](https://learn.microsoft.com/en-us/windows/package-manager/winget/uninstall)) |
| apt | `apt-get remove` (keeps configuration) or `purge`; `-s` simulates | `-y` | yes | refuses Essential packages even with `-y` ([man](https://manpages.debian.org/trixie/apt/apt-get.8.en.html)) |
| dnf | `dnf remove` (also removes dependents) | `-y` | exit 0, 1, 3, 200 | `protected_packages`, running kernel protected |
| pacman | `pacman -R` / `-Rs` | `--noconfirm` | exit status | keeps changed configuration as `.pacsave` |
| Flatpak | `flatpak uninstall` with `--user` or `--system` | `-y`, `--noninteractive` | exit status | keeps `~/.var/app` unless `--delete-data`; errors if the ref is in both installations |
| Snap | `snap remove` | yes | exit status | keeps a snapshot for 31 days unless `--purge` |

Privilege requirements for apt, dnf, pacman, Snap, system Flatpak and system units were
not stated in the pages read (**unconfirmed**); in practice they need root or polkit.
None of these mechanisms can be undone by Lumen. dnf's `history undo` and Snap's
snapshots are partial and are not offered as Lumen guarantees.

## Lumen's existing building blocks

From a survey of `develop` at 4edb6bd:

- **Domain:** `Application`, `AppId` (BundleId, MsixFamily, MsiProductCode,
  UninstallKey…), `Service` (launch agents, daemons, login items, Windows services,
  drivers, tasks, startup entries), and `Package`/`PackageManager`, which no adapter
  fills yet (`crates/lumen-domain/src/inventory.rs`).
- **Ports:** `InventorySource` returns an `InventoryReport` with a `SourceStatus`
  (complete, partial, suspected truncated, unavailable): coverage is already explicit.
- **Gaps:** Uninstall metadata (`UninstallString`, `QuietUninstallString`,
  `WindowsInstaller`, `SystemComponent`) is not read. MSIX, receipts, Homebrew and all
  of Linux are missing. Collected services are never linked to applications: there is
  no Service→Application relationship. There is no software view, page or command.
- **Precedent for running a vendor command:** ADR-0049 and `crates/lumen-app/src/tools.rs`.
  The program and arguments come only from the knowledge base. The command runs with no
  shell, closed stdin, its own process group and a time limit, keeps a bounded output
  tail, and runs only after the page showed the exact command and the user confirmed it.
  The desktop refuses to run it elevated (`refuse_privileged`, ADR-0005).
- **Policy:** application state is always kept (ADR-0039). There is no rule yet for
  launch agents, services or security software (roadmap 7.3b open).
- **AI:** Jev receives redacted metadata only. ADR-0059's usage signals are never sent.

## Threats and safety constraints

| Threat | Constraint |
| --- | --- |
| Command injection through uninstall metadata (`UninstallString` is installer-written text) | never run it through a shell; parse it into a program and arguments; accept only forms Lumen recognises (an MSI product code, a quoted executable path inside the product's install location or a known uninstaller location); anything else is "removal not supported here" |
| Wrong target (display names collide, two installations) | identify software by its platform-native ID and source; refuse when two records share it ambiguously |
| Scope creep (dependents, shared runtimes, `--zap`, `purge`) | preview what the mechanism also removes; default to the mechanism that keeps user data; never add options the user did not see |
| Privilege escalation | ADR-0005: v1 runs as the user. Removals that need administrator or root rights are listed as such and not run until a privileged helper exists; never auto-approve elevation |
| Removing protective or managed software | security software, enterprise-managed apps, OS components, drivers, and anything the platform marks non-removable are shown but not removable from Lumen |
| AI influence | Jev may explain purpose and impact from metadata; it never selects a target, mechanism or argument, and never makes a removal less cautious (ADR-0020) |
| False success | a removal succeeds only when a fresh inventory no longer lists the identity; exit codes and leftovers are reported as observed |
| Privacy | the software list is personal: it stays on the device; only what the user opts into reaches Jev, redacted |
| Irreversibility | the preview says plainly that removal cannot be undone by Lumen |

## Recommendations

1. **#806 inventory first, read-only.** Fill `Package` and extend `Application` with
   install source and uninstall metadata as untrusted text. Add sources one at a time:
   - Windows Uninstall keys (extend `InstalledApps`) and MSIX;
   - macOS receipts and Homebrew;
   - Linux dpkg, rpm, pacman, Flatpak and Snap, each only when its tool is present
     (capability-based, `SourceStatus::Unavailable` otherwise).

   Show the list before any removal exists.
2. **#807 relationships.** Add Service→Application links where evidence supports them:
   bundle ID, signing team, program path inside the install location, or package
   ownership of the file. Never link by display name.
3. **#808 Jev explanations.** Extend the request with a closed vocabulary for software
   and services, redacted like paths. Keep usage signals out (ADR-0059).
4. **#809 removal.** One mechanism per platform, the safest first:
   - Windows MSI by product code;
   - macOS move-to-Trash of a user-owned app bundle with no privileged components;
   - Linux per-user Flatpak.

   Registered `UninstallString` commands come later and only in recognised forms.
   Everything needing elevation waits for the privileged helper (ADR-0005).
5. **#810 leftovers.** Reuse the existing file pipeline: data left behind by a removed
   application already becomes an orphan (`ApplicationPresent = false`), which the
   policy sends to review (ADR-0039). No new deletion path is needed.

## Open questions for the owner

- Should security software and enterprise-managed applications be listed but never
  removable (recommended), or removable after an extra warning?
- Is a privileged helper (ADR-0005) in scope for this epic, or do removals that need
  administrator or root rights stay "show how to remove it yourself"?
