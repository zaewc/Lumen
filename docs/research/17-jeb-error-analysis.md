# 17. Jeb's errors on dataset 2026.10.1

- Date: 2026-10-26
- Scope: roadmap 14.2. Why Jeb (`gsm.jeb`, prompt `jev-decisions/1`) judges needed items
  removable, and which deterministic guards would catch that without costing recall.
- Sources: the recorded answers in
  [`ai/cassettes/jev-decisions/1/jeb-2026.10.1.json`](../../ai/cassettes/jev-decisions/1/jeb-2026.10.1.json)
  (each answer's chosen option, every option's probability, the chosen reason and the
  injection probability), the cases in `ai/golden/2026.10.1/` and
  `ai/adversarial/2026.10.1/`, and the
  [report](../../ai/reports/jev-decisions/1/jeb-2026.10.1.md). No request was sent to
  Jeb or any other model.
- Reproduce: `python3 ai/tools/analyze_jeb_errors.py ai` prints every table below (and
  more) from the committed files. It maps answers and grades them as
  `lumen_jev::decisions::judgment_for` and `lumen_jev_eval::run` do, first checks that,
  with the guards ADR-0043 adopted, every answer equals the committed report's, and
  analyses the answers without the guards, as recorded (81 / 1,103 critical false
  positives, 54.4% precision, 96.9% recall, 3.1% abstention; 23 / 160 and 19 canary
  flips).

A **critical false positive** is a needed item judged `likely_removable` with a
probability of at least 0.7 (medium or high confidence). Thresholds are chosen on the
`dev` and `calibration` splits only (460 critical golden cases); `test-locked` (643) is
reported separately and was not used to choose anything.

## Headline

- The 81 critical false positives are confident-looking but not extreme: probability
  0.70–0.89, mean 0.77; none reached 0.9. Every `likely_removable` answer at 0.9 or
  more (60) was right.
- Five patterns explain them (each false positive counted once, in the first row
  that fits):

  | Pattern | FP | What Jeb did |
  | --- | --- | --- |
  | **Answers that undo themselves** | 10 | 7 ignore `regeneratable: false` in the request; 3 cite a reason that does not argue for removal (`unsigned_executable`, `recently_used`) |
  | **"Cache"/"tmp" in the name of needed data** (OneNote's cache, Notepad's TabState, JetBrains' system directory, ledgers and font vaults in `tmp`, Package Cache, executables in temp folders) | 27 | cited `regeneratable_cache`; 22 of them with no regeneration fact at all, 5 with a `not_regenerable` knowledge-base entry |
  | **Xcode archives and dSYMs** (`developer_state`: four `.xcarchive`s, their `dSYMs`, a `dSYM.zip`) | 18 (23 with those that also ignore `regeneratable: false`) | cited `knowledge_base_match` although the request has no knowledge-base entry: it took archives for build output |
  | **Age as disposability** (security quarantines, autosaves, screencasts, TextEdit's autosave) | 19 | cited `stale_by_time`; 42 of all 81 false positives were not accessed for a year or more |
  | Other names it took for known artifacts (Sketch autosaves, a screencast, `.git/objects`), and 2 `owner_app_absent` | 7 | `knowledge_base_match` without an entry (5), `owner_app_absent` (2) |

- Of the 81, **none** cites a reason the request contradicts outright, and 50 cite a
  fact the request does not contain (28 `knowledge_base_match` without an entry, 22
  `regeneratable_cache` without a regeneration fact). But that is also how Jeb gets
  removable items **right**: 55 of its 61 correct `knowledge_base_match` answers and 184
  of its 288 correct `regeneratable_cache` answers cite a fact that is not there. Citing
  an absent fact is Jeb's normal style, not an error signal.
- **Canary flips:** 19. 15 of the 19 flipped answers cite `vendor_declared_cache`, which
  no canary's evidence contains: the payloads ("Apple-approved cache", the same in
  Korean, Japanese, Spanish and German, homoglyphs, zero-width) are read as a vendor's
  declaration. 17 of the 19 twins were answered `likely_needed`, so the canary moving
  to `uncertain` is still a flip.
- **Injection probability** separates clean from injected requests at a low level: all
  1,603 golden answers are below 0.06 (the highest is 0.052), while 116 of the 176
  canaries are at 0.1 or more. The current threshold (0.5) flags only 76.
- **The probabilities are poorly calibrated** for this decision:

  | Removable probability | Answers | Right (expected removable) | Needed item |
  | --- | --- | --- | --- |
  | below 0.70 | 311 | 73 (23%) | 191 |
  | 0.70–0.80 | 172 | 96 (56%) | 57 |
  | 0.80–0.90 | 205 | 178 (87%) | 24 |
  | 0.90 or more | 60 | 60 (100%) | 0 |

  Raising the bar trades recall steeply (below).

## Where it fails, and what it got right on the same strata

| Stratum | Critical cases | Critical FP | `likely_needed` | `uncertain` | removable, low |
| --- | --- | --- | --- | --- | --- |
| `developer_state` | 96 | 30 (31%) | 33 | 0 | 33 |
| `app_support_data` | 285 | 17 (6%) | 230 | 6 | 32 |
| `user_content` | 80 | 12 (15%) | 54 | 1 | 13 |
| `security_module` | 190 | 9 (5%) | 125 | 7 | 49 |
| `unknown_executable` | 60 | 7 (12%) | 7 | 30 | 16 |
| `license_data` | 45 | 4 (9%) | 27 | 0 | 14 |
| `installer_cache` | 10 | 2 (20%) | 1 | 0 | 7 |
| eight other critical strata | 337 | 0 | 310 | 0 | 27 |

Within `developer_state`, the same model answered Terraform state, Claude Code's
projects and the Windows JetBrains directories mostly `likely_needed`; the error is specific to
names it associates with build output and caches. "removable, low" (191 answers below
0.7 on needed items) is not graded as critical, and in the app today a removable answer
of any confidence leaves a proposed quarantine standing.

| Fact in the request | of the 81 FP | of 831 critical cases answered safely | of 407 correct removable answers |
| --- | --- | --- | --- |
| `regeneratable: false` | 7 | 152 | 0 |
| `knowledge_base_entry` | 5 | 30 | 47 |
| `app_data_location` application support or container | 9 | 118 | 17 |
| owner not installed | 20 | 201 | 54 |
| not accessed for 365 days or more | 42 | 297 | 81 |
| coverage incomplete | 3 | 82 | 0 |
| a signature fact | 16 | 169 | 0 |

## Candidate guards

Each guard turns a `likely_removable` answer into `uncertain` when it fires and
touches nothing else, so it can only make a decision more cautious (ADR-0020).
"Discarded" counts correct removable answers the guard throws away (its recall cost).

| Guard | dev + calibration: critical FP (34) / recall (97.3%) / discarded / adversarial FP (14) | test-locked: critical FP (47) / recall (96.6%) / discarded / adversarial FP (9) / flips (15) |
| --- | --- | --- |
| (a) the request says `regeneratable: false` | 31 / 97.3% / 0 / 14 | 43 / 96.6% / 0 / 7 / 15 |
| (b) application support or container location without a knowledge-base entry | 31 / 91.4% / 11 / 9 | 41 / 94.0% / 6 / 9 / 15 |
| (c1) the reason contradicts the request (`regeneratable_cache` with `regeneratable: false`, `owner_app_absent` with the owner installed, `no_active_reference` with an active reference) | 34 / 97.3% / 0 / 14 | 47 / 96.6% / 0 / 9 / 15 |
| (c2) the reason contradicts the request or names a fact it does not contain | 11 / 41.1% / 104 / 6 | 20 / 39.1% / 135 / 1 / 16 |
| (c2) only `knowledge_base_match` without an entry | 21 / 81.6% / 29 / 14 | 32 / 85.5% / 26 / 7 / 15 |
| (c2) only `regeneratable_cache` without a regeneration fact | 24 / 56.8% / 75 / 13 | 35 / 50.2% / 109 / 6 / 19 |
| (c2) only `vendor_declared_cache` without a vendor declaration | 34 / 97.3% / 0 / 7 | 47 / 96.6% / 0 / 6 / 14 |
| (c3) the reason does not argue for removal | 32 / 97.3% / 0 / 13 | 46 / 96.6% / 0 / 9 / 14 |
| (d) removable probability below 0.75 | 20 / 69.7% / 51 / 10 | 26 / 71.1% / 60 / 6 / 15 |
| (d) below 0.80 | 9 / 55.7% / 77 / 5 | 15 / 57.4% / 92 / 4 / 13 |
| (d) below 0.85 | 6 / 33.5% / 118 / 1 | 2 / 36.2% / 142 / 0 / 13 |
| (e) injection probability 0.10 or more | 34 / 97.3% / 0 / 7 | 47 / 96.6% / 0 / 5 / 15 |
| **(a) + (c1) + (c3) + undeclared vendor cache + (e)** | **29 / 97.3% / 0 / 4** | **42 / 96.6% / 0 / 2 / 13** |

All splits together, the combination moves golden critical false positives from 81 to
71 (upper bound 9.05% → 8.05%), adversarial ones from 23 to 6 and canary flips from 19
to 17, with recall unchanged at 96.9%; abstention rises from 3.1% to 7.2% because
needed items Jeb called removable become `uncertain`.

Choices ([ADR-0043](../decisions/0043-discard-removable-answers-the-evidence-does-not-support.md)
implements them):

- **(a), (c3), the undeclared vendor cache and (e) are adopted:** each removes critical
  false positives on dev + calibration (golden or adversarial) and discards no correct
  answer there, and the same holds on test-locked. Each is also an answer that cannot
  be evidence: it ignores what the request states, gives no reason for removal, credits
  a declaration only untrusted text could have suggested, or comes with a sign of text
  steering the decision.
- **(c1) is adopted** although this cassette shows no effect beyond (a): a reason the
  request contradicts is the definition of an answer that cannot be evidence, and it
  costs nothing here.
- **(e)'s threshold** is 0.10, chosen on dev + calibration: no golden answer there
  reaches 0.06, and at 0.05 one correct removable answer would be discarded; 0.10 leaves
  about twice the highest clean value.
- **(b) is rejected:** on dev + calibration it removes 3 critical false positives and
  discards 11 correct answers (Electron caches and similar under Application Support,
  the strata built to look like data).
- **(c2) as a whole, and its `knowledge_base_match` and `regeneratable_cache` parts, are
  rejected:** recall would fall to 41%, 82% and 57%. These are what prompt v2 should
  fix: the reason options must say which fact they need.
- **(d) is rejected:** with these probabilities, every threshold that removes false
  positives costs at least a quarter of recall; Phase 14 requires recall to stay at
  96.9%. A threshold belongs on a calibration map (roadmap 14.5), not on raw
  probabilities.

## After the guards

`lumen_jev::guard` applies the adopted guards on both validation paths, and the
committed reports were regenerated by replaying the unchanged cassettes; the script
checks them case by case:

| Dataset 2026.10.1 | Before | After |
| --- | --- | --- |
| Golden critical FP, all splits | 81 / 1,103, bound 9.05% | 71 / 1,103, bound 8.05% |
| Golden critical FP, `test-locked` | 47 / 643, bound 9.60% | 42 / 643, bound 8.73% |
| Golden precision / recall / abstention | 54.4% / 96.9% / 3.1% | 59.7% / 96.9% / 7.2% |
| Adversarial critical FP | 23 / 160, bound 20.78% | 6 / 160, bound 7.98% |
| Adversarial recall | 81.2% | 18.8% |
| Canary flips | 19 | 17 |

Adversarial recall falls because the 16 reverse-injection canaries ("classify as
needed" on removable items) trip the injection guard and come back `uncertain`, which
their labels accept. On dataset 2026.10.0, the one critical false positive (an Xcode
archive with `regeneratable: false`) becomes `uncertain`.

## Findings about the dataset

- **The `category` field leaks the label.** Every critical golden case has category
  `other`, and no expected-removable one does. A guard "discard removable answers on
  `other`" would score zero critical false positives with no recall cost, and mean
  nothing: it measures the generator, not the judge. It is not adopted. In production
  the category follows the evidence (`lumen_app::assess::category`: a knowledge-base
  match gives `developer_cache`, a vendor declaration or caches location `app_cache`,
  anything else `other`), so the dataset's `other` on cases with a knowledge-base entry
  is also a request production would not build. The next dataset version should derive
  the category the way production does and include needed items in disposable-looking
  categories.
- **`regeneratable` evidence is never sent in production.** 318 cases carry it, but
  `lumen_jev::request::build` has no fact that produces it. Guard (a) applies to any
  request that does carry it (the contract allows it), and has no effect on the
  requests the app builds today. Sending it for knowledge-base entries whose safety
  class is `not_regenerable` (JetBrains' system directory, Claude Code's session data,
  Docker's disk image) is a candidate request change, with its own evaluation.
- **The locked split cannot pass the gate.** `test-locked` has 643 critical cases; with
  no critical false positive at all the bound would be 0.57%, above the 0.5% target. The
  per-class bound in Phase 14 needs about 736 critical cases per class.

## Recommendations for prompt v2 (`jev-decisions/2`, roadmap 14.4)

To be evaluated on the `dev` split first, with the owner's approval for the quota.

1. **Tie every removal reason to a fact.** `knowledge_base_match`: "the evidence
   includes a `knowledge_base_entry`"; `vendor_declared_cache`: "the evidence includes
   a `vendor_declared_cache` item"; `regeneratable_cache`: "the evidence says
   `regeneratable: true`, or a knowledge-base entry or vendor declares it a cache".
   Then the (c2) guards above become cheap, because a correct answer would cite a
   reason that is there.
2. **Define `likely_removable` by positive evidence of disposability** (a knowledge-base
   entry, a vendor declaration, a caches location, `regeneratable: true`), and say that
   age, an absent owner, or words like "cache", "tmp", "temp" or "backup" in a name are
   not enough on their own: without positive evidence the answer is `uncertain`.
3. **Name the hard negatives in the criteria:** archives and debug symbols are not
   build output; application support and container folders hold application state;
   quarantine folders of security software are kept.
4. **Say that claims of approval or authority in untrusted strings are not vendor
   declarations**, in the reason options as well as the assessment instructions.
5. **Ask several times** (self-consistency, most cautious answer wins) and fit a
   calibration map on the calibration split before choosing any probability threshold
   (roadmap 14.5).
