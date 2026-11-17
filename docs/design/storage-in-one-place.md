# Storage in one place (roadmap 16.11)

- Status: **Direction and name accepted by the owner (2026-11-18)**; the rest is the
  lead's design, open to the owner's revision. Decision record:
  [ADR-0069](../decisions/0069-show-storage-in-one-place-grouped-by-owner.md).
- Replaces: [apps-in-explore.md](apps-in-explore.md) (Option A, shipped in 0.13.0, is
  the first step towards this).
- Builds on: [ui-principles.md](ui-principles.md), [ux-writing.md](ux-writing.md),
  [ADR-0039](../decisions/0039-ask-for-review-only-where-a-person-can-act.md),
  [ADR-0049](../decisions/0049-run-tool-commands-on-request.md),
  [ADR-0051](../decisions/0051-quarantine-review-items-the-user-chooses.md),
  [ADR-0052](../decisions/0052-restrained-navigation-without-glass.md),
  [ADR-0062](../decisions/0062-uninstall-macos-apps-into-quarantine.md),
  [ADR-0065](../decisions/0065-compare-completed-scan-roots-for-identical-copies.md),
  [ADR-0068](../decisions/0068-recognise-a-removed-apps-data-only-with-corroboration.md)

## Problem

Three places answer one question, "what is using my disk, and what can I safely get
back?", each from Lumen's internal point of view:

| Place | Shows | Acts |
| --- | --- | --- |
| 저장 공간 (Storage) | The disk band, the last scan's counts, tool storage | Scan |
| 탐색 (Explore) | The policy's items by kind (developer, app cache, …), the map, apps ≥ 100 MB | "정리하기…" moves every proposal; one review item at a time from its sheet |
| 앱 (Apps) | Installed software by name, with facts and AI notes | One app at a time |

So the person has to know Lumen's mechanisms (policy plan, review choice, app move,
tool command) to find their space. Xcode is the clearest case: the app is on Apps,
its DerivedData under "개발 도구" in Explore, and its simulators maybe under "검토
필요". Nobody sees "Xcode uses 65 GB".

The owner asked for one flow (2026-11-08) and, after Option A shipped, for one
consistent experience even if Explore is rebuilt (2026-11-18).

## The model: owners, parts, one selection

**One page, 저장 공간.** The navigation becomes 저장 공간 · 격리함 · 설정
(ADR-0069). Explore and Apps are folded into it; their routes lead there.

**An owner** is something a person recognises: an installed app, a developer tool, a
package manager, an app that was removed (ADR-0068), or, when nothing owns an item,
its kind ("기타"). Owners come from data the host already sends:

- an item's `group.source` (a tool such as `Xcode`, or an app's bundle ID) and
  `group.kind`;
- the software list (name, bundle ID, size from the scan, facts, AI note, services);
- for packages, their manager (Homebrew and the rest) as one owner each.

**A part** is one thing under an owner: the app itself (앱 본체), or one of the
policy's items with its own verdict. An owner's row shows its total and what can be
cleaned up, never one verdict for the whole owner:

```
 저장 공간                                    [다시 검사 ⌘R]  [정리하기 · 3개 44.6 GB…]
 ──────────────────────────────────────────────────────────────────────────────
 ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░   412 GB 사용 중 · 82 GB 남음
 └──── 살펴본 항목 186 GB ────┘
 항목 213개를 살펴봤어요 · 52 GB를 정리할 수 있어요

 [전체] [정리 제안] [명령으로 정리] [검토 필요] [유지]               [검색 ⌘F]
 ┌──────────────────── map: one tile per owner ──────────────────────────────┐
 └────────────────────────────────────────────────────────────────────────────┘
 ▾ Xcode                               65.0 GB · 38.3 GB 정리할 수 있어요
     [ ] 앱 본체           /Applications/Xcode.app                 14.8 GB
     [✓] DerivedData       정리 제안                                30.1 GB
     [ ] 시뮬레이터         검토 필요                                  8.2 GB
         설정과 상태        유지                                      0.4 GB
     앱 정보 보기 ›
 ▸ pnpm                                12.1 GB · 명령으로 정리
 ▸ Steam                               최소 9.4 GB
 ▸ 지운 앱이 남긴 데이터  3개             2.3 GB · 검토 필요
 ▸ 기타                                 4.4 GB · 2.4 GB 정리할 수 있어요
 ▾ 똑같은 파일  묶음 12개                  6.1 GB 중복
     tutorial.mov  2개                                                   1.8 GB
       [ ] ~/Downloads/tutorial.mov
       [ ] ~/Movies/tutorial.mov
```

**One selection, one primary action.** Checkboxes mark what will move; the toolbar's
only primary button names it: "정리하기 · 3개 44.6 GB…". Its total is what moves into
Quarantine, never "space freed" (space comes when it is removed for good).

| Part | Checkbox | Starts | Why |
| --- | --- | --- | --- |
| 정리 제안 (Proposed) | Yes | Checked | Today's "정리하기…" moves every proposal; unchecking only makes the plan smaller |
| 검토 필요, choosable (ADR-0051) | Yes | Unchecked | A person's explicit choice, recorded as such |
| 앱 본체, a macOS app (ADR-0062) | Yes | Unchecked | Never preselected or selected in bulk; checking it asks for a fresh preview |
| A copy in a group of identical files (ADR-0065) | Yes, all but one per group | Unchecked | Lumen never picks a copy; at least one always stays |
| 명령으로 정리 | No | — | Cannot be undone; runs from its sheet after its own confirmation (ADR-0049) |
| 유지, other review items, protected software | No | — | Lumen leaves it alone |

There is no owner-level or "select all" checkbox: it would select review items and
apps in bulk.

**Checking an app** asks the host for that app's preview right away (as the move
dialog does today). A refusal (running, another account, App Store, …) shows on the
row with its reason and unchecks it; "종료하기" (16.3a) stays available there.

**Checking a copy.** Copies are grouped as the comparison found them, largest group
first, under one "똑같은 파일" owner. A person checks the copies to move; once all but
one are checked, the last one's checkbox is disabled ("하나는 남겨 둬요."). Hard links
and copies the comparison skipped have no checkbox (moving them frees nothing or is
not verified). The page sends only the comparison's group and copy IDs, never a path.
Just before moving, the host reads the chosen copy and a kept copy of the same group
again, by handle, and moves the chosen one only if both still have the compared
contents; otherwise that copy is refused alone. A copy is still moved into
Quarantine, never deleted, and is listed there as chosen by the person.

**Confirmation.** The button opens the host's confirmation window (FS-1), which lists
four groups: "Lumen이 제안한 항목", "직접 고른 항목", "직접 고른 앱" and "직접 고른
똑같은 파일" (each copy with the copy that stays), each app with
its preview facts (its `~/Library` data stays; Quarantine restores it). One confirm
moves them all. Just before moving, every item and every app is checked again by its
own gate; an app that fails is refused alone and the report says why. Doing this needs
the host to confirm the plan and the apps' previews together (ADR-0069, amending
ADR-0062 rule 4).

## States

- **Before any scan.** The disk band, then the installed apps as owners. Sizes come
  from the latest completed scan in `index.db`, labelled "지난 검사 기준"; with no
  scan at all they are left out, never shown as zero. Apps can be checked and moved
  without scanning. The toolbar's primary is "디스크 검사".
- **Scanning.** The progress panel takes the list's place, as today.
- **Results.** As drawn above. Owners are ordered by total size, largest first; the
  status filter shows only the owners with parts of that status, and only those parts.
- **After a clean-up.** The report, then what is left of the scan without checkboxes
  and "더 옮기려면 다시 검사하세요." (#872): the core plans from the scan as it was.
  Apps stay checkable, since each app has its own fresh preview.
- **Browser dashboard.** No software inventory: owners come from the scan's items
  only; no app parts.
- **Windows and Linux.** Owners from the scan and the software list; no app part is
  checkable until a removal mechanism exists (16.3h, 16.7).

## What moves where

| Today | Becomes |
| --- | --- |
| Overview's disk band and the last scan summary | The top of 저장 공간 |
| Overview's quarantine row | The navigation's held count (`nav.held`) |
| Overview's tool storage | The tool owner's details |
| Explore's kind sections and source groups | Owner rows |
| Explore's map | The same map, one tile per owner |
| Explore's Apps section (16.2) | App owners |
| A review item's "격리함으로 옮기기…" (15.10) | Its checkbox |
| Apps page rows, search, facts, AI notes, services | App owners, the page's search, the owner's details ("앱 정보 보기") |
| Apps page packages | The package manager's owner and its details |
| Identical copies (ADR-0065), a separate read-only section | The "똑같은 파일" owner: groups as parts, copies checkable by the person |

## Key strings

Korean in 해요체 ([ux-writing.md](ux-writing.md)); "정리" and "정리할 수 있어요" stay
Lumen's words.

| Use | Korean | English |
| --- | --- | --- |
| Page and navigation | 저장 공간 | Storage |
| Primary, with a selection | 정리하기 · 3개 44.6 GB… | Clean up · 3 items, 44.6 GB… |
| Primary, nothing selected | 정리하기… (disabled: 고른 항목이 없어요) | Clean up… (disabled: Nothing is selected) |
| Owner total | 65.0 GB · 38.3 GB 정리할 수 있어요 | 65.0 GB · 38.3 GB can be cleaned up |
| The app part | 앱 본체 | The app itself |
| App details link | 앱 정보 보기 | App details |
| Sizes from an earlier scan | 지난 검사 기준 | As of the last scan |
| Confirmation groups | Lumen이 제안한 항목 / 직접 고른 항목 / 직접 고른 앱 | Proposed by Lumen / Chosen by you / Apps chosen by you |
| App refused | 이 앱은 지금 옮길 수 없어요: {이유} | This app can't be moved now: {reason} |
| Last copy of a group | 하나는 남겨 둬요. | One copy always stays. |
| Copy refused | 내용이 바뀌어서 옮기지 않았어요. | Not moved: its contents changed. |

## Implementation (one PR each)

1. `docs`: this design and ADR-0069 (this PR).
2. `feat(app)`: plan the proposals minus the ones a person left out, with chosen
   review items; leaving out only makes a plan smaller. API, CLI and transport.
3. `feat(web)`: the owner model, a pure function of the candidates and the software
   list, with tests.
4. `feat(web)`: 저장 공간 at `/`: Overview and Explore in one page with owner rows;
   `/clean` leads there. "정리하기…" behaves as before.
5. `feat(web)`: checkboxes for proposed and choosable review parts; the button names
   the selection and plans it (step 2).
6. `feat(desktop)`: one confirmation for a plan and app previews (ADR-0062 rule 4 as
   amended), each re-checked before it moves.
7. `feat(web)`: the app part's checkbox, with its preview on checking.
8. `feat(app)`: chosen copies of identical files in a plan, by comparison IDs, each
   re-read with a kept copy before it moves; then their checkboxes on the page.
9. `feat(web)`: before-scan state, search (⌘F) and app details; the Apps page and its
   navigation item go; `/apps` leads to 저장 공간.
10. `feat(web)`: the map with one tile per owner.
11. `docs`: glossary (탐색 retired), ui-principles and screenshots.

The Apps page stays until step 9, so no release loses a feature midway.

## Decisions

- **Direction (owner, 2026-11-18):** one page for storage, owners with parts, one
  selection with one primary action.
- **Name (owner, 2026-11-18):** 저장 공간.
- **Identical copies like apps (owner, 2026-11-18):** a person may move copies they
  check; Lumen never picks one, and one copy of every group always stays.
- **App part checkable (lead's recommendation, open to the owner):** yes, never
  preselected. Without it, apps keep their own dialog and the page has two ways to
  move things.
