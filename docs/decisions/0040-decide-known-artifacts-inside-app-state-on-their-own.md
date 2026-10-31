# ADR-0040: Decide known artifacts inside an app's state on their own

- Status: Accepted
- Date: 2026-10-26
- Amends: roadmap 10.8 (candidates are the outermost identified entries)
- Amended: 2026-10-31 (a cache folder that holds a browser profile)

## Context and problem statement

A candidate is a scanned entry that some evidence identifies: a knowledge-base match,
an application data location or a vendor's cache declaration. Until now only the
outermost candidates were decided, so a candidate inside another was decided with it.

Electron and Chromium apps keep their regenerable caches (`Cache`, `Code Cache`,
`GPUCache`) inside their `~/Library/Application Support/<App>` folder. Lumen links that
folder to the installed app and keeps it as the app's own data (`classify.app-state`,
[ADR-0039](0039-ask-for-review-only-where-a-person-can-act.md)). A knowledge-base entry
for the caches inside it could never apply: on the owner's Mac, gigabytes of Chromium
caches in Claude, Visual Studio Code, Discord, Notion and Postman were only part of
whole-app items that Lumen leaves alone.

## Decision drivers

- Safety: an app's state holds the user's data; deciding a sub-folder on its own must
  never put that state, or the same bytes twice, into a plan.
- Honest sizes: space is counted once.
- The smallest change that keeps the policy and the executor unchanged.

## Considered options

1. **Decide knowledge-base matches inside an app's state on their own**, and decide the
   state for the rest of it.
2. Let the app-state rule propose known sub-folders itself: the policy would need the
   knowledge base's paths, and one decision would cover several objects.
3. Keep outermost-only selection.

## Decision outcome

Chosen option: **1**.

- A knowledge-base match is a candidate of its own when every candidate enclosing it is
  identified only as an application's state: an `AppDataLocation` other than `Caches`
  (support folder, container, preferences, saved state, web data).
- A match inside anything else (another match, an app's `Caches` folder, a declared
  cache) is still decided with its outer item, as before.
- The enclosing state is decided as before (`KEEP` under ADR-0039, `REVIEW` when it is
  orphaned), from what the scan saw of it without the nested candidates (`nested`,
  `Subtree::carve`): its size and completeness are those of the rest of the folder.
- State is never proposed for quarantine (only the knowledge-base and app-cache rules
  propose it), so a plan can never hold both an app's state and a folder inside it.
  Confirmation re-decides each planned item from its own subtree, as before.
- The nested item is decided by its own evidence like any other match: a regenerable
  match is proposed whatever its size (a classification rule fires, so ADR-0039's
  `default.keep-small` does not apply), and an item a running app holds open is kept.

### Amendment: a cache folder that holds a browser profile (2026-10-31)

Apps that embed Chromium can keep a whole browser profile in their `Caches` folder:
on the owner's Mac `~/Library/Caches/com.spotify.client` holds `Default/` and
`Browser/` profiles with `Cookies`, `Login Data`, `Web Data` and `Local Storage`, and
Adobe CEP panels keep `Cookies` and `Local Storage` under
`~/Library/Caches/CSXS/cep_cache/<panel>`. `classify.app-cache` proposed such a folder
for quarantine, which would have moved the user's sign-ins and site data and, after
retention, removed them.

- On macOS, a direct child of `~/Library/Caches` with an entry named `Cookies`,
  `Login Data`, `Web Data`, `Local Storage`, `Session Storage` or `IndexedDB` at most
  three levels below it gets `BrowserProfile` evidence (source `macos.web-profile`;
  names only, compared exactly; nothing is opened). Three levels cover a profile in
  the folder itself, a profile folder (`Default/Cookies`), and a profile's `Network`
  folder or a profile one level deeper (`cep_cache/<panel>/Cookies`); deeper names are
  unrelated content, such as a package named `cookies` in a package manager's cache.
- `classify.browser-profile` keeps any subject with that evidence (risk: sensitive
  data), whoever owns it, and `classify.app-cache` does not fire for it. The change
  only moves verdicts towards `KEEP`. The policy version moves to 0.14.0.
- Such a folder is always kept, so selection treats it like state: a knowledge-base
  match inside it (for example a vendor-documented cache sub-folder) is decided on its
  own, and the folder is decided for the rest.

### Consequences

- Good: per-app knowledge-base entries can name caches inside app folders, each with
  its own sources, and the policy decides them like any other match.
- Good: a `not_regenerable` match inside a container (Docker Desktop's disk image) is
  now shown on its own, with its tool's command.
- Bad: one more rule in candidate selection. Its invariant (state is never proposed) is
  stated in `select`'s documentation and covered by tests.

## More information

- [ADR-0014](0014-deterministic-safety-policy-engine.md): the policy.
- [ADR-0021](0021-developer-artifact-knowledge-base.md): the knowledge base.
- [ADR-0039](0039-ask-for-review-only-where-a-person-can-act.md): app state is kept.
- `crates/lumen-application/src/assess.rs` (`select`, `nested`),
  `crates/lumen-app/src/assess.rs` (`Subtree::carve`).
