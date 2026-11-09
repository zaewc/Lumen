# Apps inside the scan (roadmap 16.2)

- Status: **Proposal, waiting for the owner's choice** (2026-11-09). Nothing here is
  built until the owner picks an option and answers the questions at the end.
- Roadmap: [Phase 16, item 16.2](../architecture/roadmap.md#phase-16-installed-software-after-0100)
- Builds on: [ui-principles.md](ui-principles.md), [ux-writing.md](ux-writing.md),
  [ADR-0039](../decisions/0039-ask-for-review-only-where-a-person-can-act.md),
  [ADR-0051](../decisions/0051-quarantine-review-items-the-user-chooses.md),
  [ADR-0052](../decisions/0052-restrained-navigation-without-glass.md),
  [ADR-0053](../decisions/0053-show-the-evidence-behind-each-candidate.md),
  [ADR-0060](../decisions/0060-list-software-and-remove-it-only-through-its-own-mechanism.md),
  [ADR-0062](../decisions/0062-uninstall-macos-apps-into-quarantine.md),
  [ADR-0063](../decisions/0063-use-administrator-rights-through-an-opt-in-helper.md)

## Problem

After trying 0.10.0 the owner asked for one flow instead of two pages (2026-11-08). To
recover space today a person has to know that there are two places:

- **Explore** (탐색) scans, lets the policy decide, and shows the results as a map and
  a list by group. Its one primary action, "정리하기…" (Clean up…), plans every item
  the policy proposes, asks once, and moves that plan into Quarantine. After a
  clean-up, the rest of the scan stays listed, with "To move more, scan again" (#872).
- **Apps** (앱) lists installed software by name. A user-owned macOS app has its own
  "격리함으로 옮기기…" (Move to Quarantine…), which asks the host for a fresh preview
  of that one app and confirms it by hash (ADR-0062). A running app can be asked to
  quit from the dialog (16.3a).

Large apps are often where the space is (Xcode, games, creative suites), but Explore
never mentions them, and the Apps page is sorted by name and shows no size at all
(the size appears only inside the preview).

The two actions cannot simply merge:

- An app uninstall is **not** part of the scan's plan. It has its own deterministic
  gate (ADR-0062 rule 1), is previewed per app by key, confirmed by its own hash, and
  refused while the app runs or when a background item may run it. The cleanup policy
  is never asked about it (rule 3).
- **Apps owned by another account** (`.pkg` installs, App Store) are refused today
  (`not_owned_by_user`) and become movable only through the helper (ADR-0063,
  roadmap 16.3c–e).
- **An app's own data in `~/Library`** stays when the app moves (ADR-0062 rule 4) and
  is decided in Explore after the next scan (ADR-0039). A per-app total of that data
  is linked only after 16.9.
- **One primary action per page** ([ui-principles.md](ui-principles.md)): Explore
  already has "정리하기…"; a second primary button would compete with it.

So the question is how a scan's results can show the apps that take space, largest
first, with the existing per-app move, without blurring which action moves what.

## What the host knows today

Checked in the code at `8638d3c`:

| Question | Answer |
| --- | --- |
| Does the software list carry sizes? | **No.** `SoftwareItemView` (`crates/lumen-api/src/views/software.rs`) has identity, origin, location, protection, services and the AI note, no size. |
| Where does the size in the move dialog come from? | `lumen-app::uninstall::measure` walks the bundle live when a preview is made (up to 30 s, `MEASURE_LIMIT`), giving `size_bytes` and `size_complete`. Doing that for every app on every scan would be slow and repeat the scan's work. |
| Does the scan see app bundles? | **Yes, for a whole-disk scan on macOS:** the roots are the data volume's firmlinks, which include `/Applications` (`crates/lumen-app/src/roots.rs`). `~/Applications` is under `/Users`. A scan of chosen folders may include neither. |
| Is each file's size kept? | **Yes.** `index.db` keeps every entry of the latest completed scan with `logical`, `allocated` and `private` sizes, and the listings with completeness (`crates/lumen-store-sqlite/src/index.rs`). `entries_within` / `listings_within` read a subtree by index seek. |
| Does the assessment know where apps are? | **Yes.** `Assessment::apps` holds each installed app's ID and install location from the bundle inventory (`crates/lumen-app/src/assess.rs`, `app_locations`). macOS only; other platforms return none yet. |
| Is an app bundle a candidate? | No. Nothing names `/Applications/X.app` as a candidate, so apps never appear in the results today. |
| Does the graph link `~/Library` data to apps? | Partly: `lumen-platform-macos::support` adds `owned_by` edges for `~/Library` items named after an installed app ([evidence graph](../architecture/evidence-graph.md)). Only items that are candidates reach the UI, and app state is `KEEP` (ADR-0039). A per-app total of its data is 16.9's work. |

**Conclusion.** The host can give each installed macOS app a size **from the scan
that just ran**, with no new filesystem IO: the sum of the entries under its install
location in `index.db`, computed with the same estimate the preview uses
(`ReclaimEstimate::for_selection`), and complete only when every listing below it was
complete. A store query that sums a subtree in SQL avoids loading hundreds of
thousands of entries for a bundle like Xcode. Three states are needed, never a zero
for unknown (ADR-0008):

- `complete`: every folder in the bundle was read;
- `at_least`: some folder could not be read, so the size is a lower bound;
- `not_scanned`: the bundle is outside this scan's roots (or appeared after it).

The preview keeps measuring live and stays the authority: the list's size is display
only and is never sent back.

## Options

All three options keep these rules:

- The list never decides eligibility. Every move opens the existing dialog, which asks
  the host for a fresh preview (ADR-0062) and is confirmed by its own hash. The page
  sends only the app's key and the preview ID.
- "정리하기…" plans only the policy's proposed items, as today. No app enters a plan,
  and no app is preselected or selected in bulk.
- Protected software (system apps, security software, ADR-0060 rule 4) is never
  offered a move.
- The browser dashboard has no software inventory, so it shows no apps, as the Apps
  page does today.
- macOS only at first: it is the only platform with a move mechanism for apps
  (ADR-0062). Windows programs follow 16.3h and 16.7.

### Option A: an "Apps" section in the results list

The results list gets one more section, **after** the policy's sections, listing the
installed apps the scan measured, largest first. A row opens the existing per-app move
dialog. The section has its own heading and total, and one short line saying apps are
checked one at a time.

```
 탐색                                    [다른 폴더 검사] [다시 검사 ⌘R] [정리하기…]
 ───────────────────────────────────────────────────────────────────────────────
 항목 213개를 살펴봤어요
 18.4 GB를 정리할 수 있어요 · 항목 41개

 [전체] [정리 제안] [명령으로 정리] [검토 필요] [유지]            ← status filter
 ┌──────────────────────────── map (policy items only) ─────────────────────────┐
 │  개발 도구 ▒▒▒▒▒▒▒▒▒▒   앱 캐시 ▒▒▒▒▒   기타 ▒▒                               │
 └──────────────────────────────────────────────────────────────────────────────┘
 이름                  위치                         상태         크기
 명령으로 정리  항목 6개                                           29.0 GB
   pnpm store          ~/Library/pnpm               명령으로 정리   12.1 GB
   …
 개발 도구  항목 22개                                              14.2 GB
   …
 앱 캐시  항목 19개                                                 4.2 GB
   …
 ───────────────────────────────────────────────────────────────────────────────
 앱  10개                                                         61.3 GB
 앱은 하나씩 확인한 뒤 격리함으로 옮겨요.
   Xcode               /Applications                               14.8 GB
   Final Cut Pro       /Applications    다른 계정의 앱               5.9 GB
   Steam               ~/Applications                     최소 3.1 GB
   Figma               /Applications                                1.2 GB
   …
   설치된 앱 모두 보기                                    ← link to the Apps page
```

- **One primary action:** the toolbar keeps "정리하기…" as the only primary button.
  App rows are rows, like candidate rows: Enter, Space or a click opens a dialog, and
  that dialog's primary is "격리함으로 옮기기". No per-row button competes with the
  toolbar.
- **Status filter and map:** apps have no verdict, so they are not in the legend, the
  map or the "can clean up" total. The section shows only under "전체" (All); a status
  filter is about the policy's verdicts.
- **Size:** the section lists the largest apps (proposal: up to 10, at least 100 MB,
  the same floor as ADR-0039's unrecognised items), with "설치된 앱 모두 보기" for the
  rest on the Apps page.
- **Running apps:** the row does not claim the app runs (that changes from second to
  second). The dialog checks freshly, says "앱이 실행 중이에요…", and offers
  "종료하기" (16.3a), then checks again.
- **Other-account apps:** listed, because they take space and the person can act on
  them, with a muted "다른 계정의 앱" (Installed by another account). Until 16.3c–e
  the dialog refuses with `not_owned_by_user` and says how to remove it. After 16.3e
  the dialog says Lumen's helper moves it, and sets the helper up the first time.
  Whether this hint is shown needs a cheap owner check from the host; otherwise the
  dialog alone says it.
- **Apps with no size:** an app outside the scan's roots is not given a made-up size.
  The section ends with one line: "검사하지 않은 위치에 앱이 3개 더 있어요." (3 more
  apps are in places this scan did not read.) with the Apps link. A partly read bundle
  shows "최소 3.1 GB" (At least 3.1 GB) and sorts by that lower bound. When the scan
  covered no app location at all (chosen folders only), the section is left out.
- **After a clean-up:** the done view keeps the rest of the scan (#872). The apps
  section stays and still works, because each move makes its own fresh preview and
  does not depend on the scan's plan. A moved app leaves the section when the software
  list is read again, and the move's result appears above the list, as on the Apps
  page ("Xcode 앱을 격리함으로 옮겼어요.").

### Option B: an "Apps" view inside Explore

The results step gets a segmented control under the heading: "정리 항목" (Items to
clean up) and "앱" (Apps). The Items view is today's results. The Apps view is the
software list sorted by size, with the same rows and dialog.

```
 탐색                                    [다른 폴더 검사] [다시 검사 ⌘R] [정리하기…]
 ───────────────────────────────────────────────────────────────────────────────
 항목 213개를 살펴봤어요
 18.4 GB를 정리할 수 있어요 · 항목 41개

 ( 정리 항목 | 앱 61.3 GB )                                 ← segmented control
 ───────────────────────────────────────────────────────────────────────────────
 앱은 하나씩 확인한 뒤 격리함으로 옮겨요.
 이름                  위치                                         크기
   Xcode               /Applications                               14.8 GB
   Final Cut Pro       /Applications    다른 계정의 앱               5.9 GB
   Steam               ~/Applications                     최소 3.1 GB
   …
   크기를 모르는 앱 3개 (검사하지 않은 위치)
```

- **One primary action:** "정리하기…" acts on the items view only. In the Apps view it
  either stays (and plans items the person cannot see), or is hidden (and the toolbar
  changes with the view). Both are weaker than A.
- **Running, other-account, no size, after a clean-up:** as in A.
- **Costs:** the apps sit behind a control people may not notice, which is close to
  today's separate page; the view repeats the Apps page; and the heading's
  "can clean up" total no longer describes everything on screen.

### Option C: keep two pages, connect them

Explore stays as it is, apart from one line under the results heading that names the
apps' total and links to the Apps page. The Apps page gains sizes from the scan and a
"크기순" (By size) order.

```
 탐색                                    [다른 폴더 검사] [다시 검사 ⌘R] [정리하기…]
 ───────────────────────────────────────────────────────────────────────────────
 항목 213개를 살펴봤어요
 18.4 GB를 정리할 수 있어요 · 항목 41개
 설치된 앱이 61.3 GB를 쓰고 있어요. 앱 열기                     ← one line, a link
 …results as today…

 앱                                                     [이름순 | 크기순] [다시 읽기]
 ───────────────────────────────────────────────────────────────────────────────
   Xcode          com.apple.dt.Xcode · 16.1          14.8 GB   [격리함으로 옮기기…]
   Final Cut Pro  com.apple.FinalCut · 11.0           5.9 GB   시스템 …
```

- **One primary action:** unchanged on both pages.
- **Running, other-account, no size, after a clean-up:** unchanged; the Apps page
  shows "크기 모름" (Size unknown) where the scan did not reach.
- **Costs:** it is still two places, which is what the owner asked to end. It is the
  smallest change and needs the same host data as A.

## Comparison

| | A: section | B: view | C: link |
| --- | --- | --- | --- |
| One flow (the request) | Yes | Partly (behind a control) | No |
| One primary action per page | Yes | Weakened (toolbar changes or misleads) | Yes |
| What "정리하기…" moves stays obvious | Yes, apps are apart and outside the filter and total | Unclear in the Apps view | Yes |
| New UI | One section, one entity move | A view and a mode | One line and a sort |
| Host data | App sizes from the scan | Same | Same |
| Apps page | Stays, the full list | Stays, partly duplicated | Stays, gains sizes |

## Recommendation

**Option A**, with apps outside the status filter, the map and the "can clean up"
total.

- It is what the owner asked for: one scan shows where the space is, including apps,
  largest first.
- It keeps one primary action. The list already works this way: rows open details;
  the toolbar acts. An app row is a row that opens a dialog with its own confirmation.
- It keeps the safety model readable. "정리하기…" still means "the policy's proposed
  items"; apps are a separate section with a line that says they are checked one at a
  time, and their total is never added to what the policy can clean up.
- It costs the least new UI: a section that reuses the list's columns and the existing
  dialog. The map and legend are untouched, so no fifth status colour is invented.
- The Apps page remains the full list (search, packages, background items, AI notes).

Choose the simpler variant at every fork: no running badge in the list (the dialog
checks freshly), no app data column until 16.9 provides it, and macOS only until
another platform has a move mechanism.

## Key strings

Korean in 해요체, following [ux-writing.md](ux-writing.md); the words in Lumen's
glossary are used as listed there. Existing strings are reused where marked.

| Where | Korean | English |
| --- | --- | --- |
| Section heading | 앱 | Apps |
| Section line | 앱은 하나씩 확인한 뒤 격리함으로 옮겨요. | Apps are checked one at a time before they move to Quarantine. |
| Link to the full list | 설치된 앱 모두 보기 | See all installed apps |
| Size, part unread (reused) | 최소 3.1 GB | At least 3.1 GB |
| Apps outside the scan | 검사하지 않은 위치에 앱이 3개 더 있어요. | 3 more apps are in places this scan did not read. |
| Other-account hint | 다른 계정의 앱 | Installed by another account |
| Dialog title (reused) | Xcode 앱을 격리함으로 옮길까요? | Move Xcode to Quarantine? |
| Running (reused) | 앱이 실행 중이에요. 앱을 종료한 뒤 다시 확인해 주세요. | This app is running. Quit it and check again. |
| Quit button (reused) | 종료하기 | Quit |
| Moved (reused) | Xcode 앱을 격리함으로 옮겼어요. | Moved Xcode to Quarantine. |
| Space note (reused) | 디스크 공간은 격리함에서 앱을 영구 삭제할 때 생겨요. | Space is freed when you remove the app for good from Quarantine. |
| Data stays (reused) | ~/Library의 앱 데이터는 그대로 둬요. 새로 검사한 뒤 탐색에서 살펴볼 수 있어요. | The app's data in ~/Library stays. After a new scan, you can look at it in Explore. |
| Option C link line | 설치된 앱이 61.3 GB를 쓰고 있어요. 앱 열기 | Installed apps use 61.3 GB. Open Apps |

The screen reader hint for the section ("↑ and ↓ move between apps; Enter opens one")
follows the list's existing hint.

## Implementation (Option A, one PR each)

1. **`feat(store)`: the size of a subtree in the latest scan.** A query in
   `lumen-store-sqlite` that sums a subtree's sizes in SQL with the inputs
   `ReclaimEstimate` needs, and whether every listing below it was complete; tested on
   fixtures, with a seek-not-sort check like the existing subtree tests.
2. **`feat(api)`: app sizes from the scan.** `lumen-app` gives each installed app in
   `Assessment::apps` a size from (1), as `complete`, `at_least` or `not_scanned`;
   `lumen-api` exposes it keyed by the software list's `key` (a new view next to the
   assessment, or a field on `SoftwareItemView`, decided in that PR against
   ADR-0011), plus an owner hint if the owner wants it shown; `just gen`, the Tauri
   command and the generated client.
3. **`refactor(web)`: the app move as an entity.** Move `MoveFlow`, `MoveDialog`,
   `useAppMove` and the quit model from `pages/apps` to `entities/` (Feature-Sliced
   Design: one page may not import another). No behaviour change.
4. **`feat(web)`: Apps in Explore's results.** The section, its states (sizes, at
   least, apps outside the scan, nothing to show), keyboard order after the policy
   rows, the move's result above the list, the done view after a clean-up; strings in
   every locale; tests for ordering, the floor, unknown sizes and the filter.
5. **`feat(web)`: sizes on the Apps page.** The same sizes and a "by size" order;
   optional, owner's call.
6. **Later, by their own items:** the helper path in the dialog for other-account apps
   (16.3c–e, no change to the section), and the app's data in `~/Library` once 16.9
   links it.

Each web PR is checked in the running desktop app on a Mac, with a whole-disk scan and
with a scan of chosen folders. Documentation updated with them: this file's status,
the roadmap row, and [storage-identity.md](storage-identity.md)'s description of
results.

## Questions for the owner

1. **Which option:** A (recommended), B or C?
2. **Which apps the section lists:** the largest 10, at least 100 MB? Or every app at
   least 100 MB, or a different floor?
3. **Apps Lumen cannot move:** list other-account apps (and apps with helpers or a
   linked background item) with a muted hint, as proposed, or list only apps the
   person can move today? Showing the hint needs the host to run the cheap ADR-0062
   checks (owner, helpers, linked background items) for the list.
4. **The app's data in `~/Library`:** after 16.9, show it next to the bundle size as a
   separate number that stays when the app moves ("데이터 2.3 GB는 그대로 둬요"), or
   leave it to the policy's own sections?
5. **The Apps page:** add sizes and a "by size" order there too (step 5), or keep it
   sorted by name?
6. **Last opened:** under ADR-0059 the assessment already asks Spotlight when an app
   that owns a candidate was last opened, as cautious evidence. Should the section show it ("마지막으로 연 날짜"), or stay with size only?
   The proposal is size only: "unused" is a claim the data cannot always back.
