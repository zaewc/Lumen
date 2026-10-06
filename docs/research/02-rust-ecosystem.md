# Rust Workspace Architecture and Ecosystem for the Lumen Core

> Researched: 2026-10-05 · Scope: toolchain/edition/Cargo workspace setup, workspace layout and hexagonal ports in Rust, async runtime patterns, errors, observability, schema/TS type generation, testing, supply chain, and local persistence for Lumen's Rust core.

## Summary

- **Toolchain:** stable Rust is **1.99.0** (2026-10-01). Use **edition 2024**, pin the toolchain with `rust-toolchain.toml` (`channel = "1.99.0"`, plus `clippy` and `rustfmt` and the mobile targets), and declare a single workspace `rust-version`. A *virtual* workspace **must set `resolver = "3"` explicitly**, because it has no edition to infer it from. Resolver 3 turns on MSRV-aware (`fallback`) dependency selection.
- **Workspace inheritance:** use `[workspace.package]`, `[workspace.dependencies]` (with internal crates listed there too) and `[workspace.lints]` (`[lints] workspace = true` in every member). Clippy: keep the default groups, enable `pedantic` with a curated allow-list (uv/ruff style), and cherry-pick `restriction` lints. Never enable all of `restriction` or `nursery`.
- **Layout:** a flat `crates/` directory with folder name = crate name, a virtual root manifest, `version = "0.0.0"` internal crates, and an `xtask` crate. This follows matklad's rust-analyzer guidance, and uv (74 crate directories under `crates/`, 73 of them `uv-*` plus `uv`, per the GitHub API on 2026-10-06; earlier draft said 91) and ruff use it.
- **Async traits:** native `async fn` in traits works for static dispatch, but **it is still not dyn-compatible** (Rust Reference, 2026). For dyn ports, use `dynosaur` 0.3 (boxes only on dyn calls) or `async-trait` 0.1.92. Use `trait-variant` for `Send` variants. Better still, keep OS-facing ports **synchronous** and generic.
- **Tokio 1.53.2** is current and **1.53.x is LTS until September 2027**. Use `JoinSet` for fan-out with results, `TaskTracker` + `CancellationToken` (`tokio-util` 0.7.19) for lifecycle, `Semaphore` for bounded parallelism, and bounded `mpsc` for backpressure.
- **`tokio::fs` is just `spawn_blocking` under the hood** per its module docs; an opt-in Linux-only `enable_io_uring()` exists on the runtime builder (`io-uring` feature) but is irrelevant to Lumen's targets. `spawn_blocking` tasks **cannot be aborted** and block runtime shutdown. The scanner should be a synchronous walker on dedicated or rayon threads that checks a cancellation flag and streams batches into a bounded channel.
- **Errors:** `thiserror` **2.0.21** for typed library errors with structured fields such as path, operation and OS code. `anyhow` 1.0.104 only in binaries. `snafu` 0.9.2 is a credible alternative when you need per-call-site context.
- **Observability:** use `tracing` 0.1.44 and `tracing-subscriber` 0.3.23 with per-layer filters and a path-redaction layer. OpenTelemetry Rust **0.33** has logs and metrics API/SDK **Stable** but **traces Beta** and OTLP exporters RC/Beta. Ship it only as an opt-in adapter.
- **Schema/types:** `schemars` **1.2.2** emits JSON Schema 2020-12 and honors serde attributes, so make it the canonical cross-boundary contract. `ts-rs` 12 is the stable choice for TypeScript. `specta`/`tauri-specta` 2.0 have been in **RC since 2023** (specta 2.0.0-rc.1 2023-07-17, tauri-specta rc.1 2023-10-04; now rc.25; corrected from "since 2024"), which is a churn risk.
- **Testing:** `cargo-nextest` 0.9.146 (no doctests), `proptest` 1.11 for policy invariants, `insta` 1.49 for decisions, explanations and schemas, `tempfile` 3.27 for fixtures, `criterion` 0.8 for benchmarks, and `cargo-fuzz` (needs nightly) for on-disk format parsers. `divan` and `loom` have stale releases.
- **Supply chain:** `cargo-deny` 0.20.2 covers advisories, bans, licenses and sources. Add `cargo-vet` 0.10.2 (`safe-to-deploy` for runtime deps), `cargo-auditable` for shipped binaries, `cargo-machete` per PR and `cargo-udeps` (nightly) weekly. The Rust blog warned in 2026-09 about targeted attacks on Rustaceans and about CI secrets leaking through cached Miri output, and in 2026-08 reported a real crates.io compromise (`arrayref`/`internment`/`append-only-vec`, malicious build-script payload) — so commit `Cargo.lock`, build `--locked`, and gate new crates.
- **Persistence:** use **`rusqlite` 0.40 with `bundled`** (SQLite 3.53.x), in WAL mode, with STRICT tables and `rusqlite_migration` 2.6 (`user_version`). Bundling guarantees the **WAL-reset corruption fix** (fixed in 3.51.3 and all later releases, including 3.53.x), which system SQLite may lack; the bundled 3.53.2 is two patch releases behind upstream 3.53.4. `sqlx` 0.9 fits the server dashboard (Postgres) and does not fit the embedded store. Diesel and SeaORM 2.0 were rejected for the core.

## Findings

### 1. Toolchain, edition 2024, resolver 3, workspace inheritance, lints

**Current stable Rust (verified 2026-10-05): 1.99.0**, released 2026-10-01 ([Rust blog index](https://blog.rust-lang.org/), [Announcing Rust 1.99.0](https://blog.rust-lang.org/2026/10/01/Rust-1.99.0/)). Recent cadence from the blog index: 1.94.0 (2026-03-05), 1.94.1 (2026-03-26), 1.95.0 (2026-04-16), 1.96.0 (2026-05-28), 1.96.1 (2026-06-30), 1.97.0 (2026-07-09), 1.97.1 (2026-07-16), 1.98.0 (2026-08-20), 1.98.1 (2026-09-03), 1.99.0 (2026-10-01) (patch releases 1.94.1/1.96.1/1.97.1 were missing from the earlier draft). rustup 1.29.0 was announced 2026-03-12. rustup 1.29.1 was announced 2026-09-01. 1.99.0 stabilizes C-variadic definitions (`VaList`), `Layout::for_value_raw` / `size_of_val_raw`, `Vec::into_parts`/`from_parts`, `String::from_utf8_lossy_owned`, and **`std::fs` `set_times` / `set_times_nofollow`** (relevant to quarantine restore: restoring mtime/atime on a moved-back file without following symlinks).

Two security notices on the blog are directly relevant to Lumen's CI: "GitHub Actions leaking secrets when Miri output is cached" (2026-09-21) and "Be alert: targeted attacks on prominent Rustaceans" (2026-09-17) ([blog index](https://blog.rust-lang.org/)). Also "Demoting i686 Windows targets to std-only" (2026-10-02): do not plan a 32-bit Windows build.

**Edition 2024 is stable** (since Rust 1.85, Feb 2025) and is the edition new crates should use. `edition = "2024"` implies `resolver = "3"`, which flips `resolver.incompatible-rust-versions` from `allow` to `fallback` ([Edition Guide: Rust-version aware resolver](https://doc.rust-lang.org/edition-guide/rust-2024/cargo-resolver.html), [Cargo resolver reference](https://doc.rust-lang.org/cargo/reference/resolver.html)).

Resolver facts that matter for Lumen ([Cargo resolver reference](https://doc.rust-lang.org/cargo/reference/resolver.html), [Workspaces reference](https://doc.rust-lang.org/cargo/reference/workspaces.html)):

| Resolver | Default for | Change | Needs |
|---|---|---|---|
| `"2"` | edition 2021 | feature unification changes (no dev/build/target feature leakage) | Cargo 1.51+ |
| `"3"` | edition 2024 | `incompatible-rust-versions` default `fallback` (prefer dep versions whose `rust-version` ≤ yours) | Rust 1.84+ |

- **A virtual workspace has no `package.edition`, so it must set `resolver = "3"` explicitly** in `[workspace]`; otherwise Cargo defaults the virtual workspace to resolver `"1"` and only emits a warning ("virtual workspace defaulting to `resolver = "1"`…"), which is easy to miss in CI logs (the Cargo Workspaces reference states the explicit-setting requirement; the warning text is from Cargo's behaviour, not quoted in the reference). uv's root manifest still says `resolver = "2"` while `[workspace.package]` sets `edition = "2024"` (see the [uv Cargo.toml](https://github.com/astral-sh/uv/blob/main/Cargo.toml)) — possibly deliberate, but it means uv does not get MSRV-aware resolution by default; a third-party issue documents exactly this ("The workspace pins `resolver = "2"`, so `cargo update` can resolve past the declared MSRV", [kage1020/Cairn#335](https://github.com/kage1020/Cairn/issues/335)).
- `fallback` is a preference, not a guarantee: if no version in the requirement range is compatible, Cargo picks an incompatible one anyway. With members on different `rust-version`s the resolver uses heuristics, so **keep one `rust-version` in `[workspace.package]`** and inherit it.
- Override per run: `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback` or `[resolver] incompatible-rust-versions = "fallback"` in `.cargo/config.toml`.

**Workspace inheritance** ([Workspaces reference](https://doc.rust-lang.org/cargo/reference/workspaces.html)):
- `[workspace.package]` inheritable keys: `authors, categories, description, documentation, edition, exclude, homepage, include, keywords, license, license-file, publish, readme, repository, rust-version, version`. Members write `edition.workspace = true`.
- `[workspace.dependencies]`: members use `foo = { workspace = true, features = [...] }`. `optional` **cannot** be set at workspace level (set it in the member); member `features` are **additive** to the workspace entry. The current Cargo reference says that for **edition-2024 packages on Rust 1.99+, a member's `default-features = false` now overrides the workspace entry** (previously a footgun where the member setting was ignored with a warning). Rely on this only if MSRV ≥ 1.99; otherwise set `default-features = false` in the workspace entry and add features per member.
- `[workspace.lints.rust]` / `[workspace.lints.clippy]` + `[lints] workspace = true` in every member. Lint-group entries need `priority` so individual lints override them, e.g. `pedantic = { level = "warn", priority = -1 }`.

**`rust-toolchain.toml`** ([rustup overrides](https://rust-lang.github.io/rustup/overrides.html)): `[toolchain]` with `channel` (or `path`), `profile` (`minimal|default|complete`), `components`, `targets`. Precedence: `cargo +toolchain` > `RUSTUP_TOOLCHAIN` > `rustup override` > `rust-toolchain.toml` > default. Pin an exact version (e.g. `channel = "1.99.0"`) together with a committed `Cargo.lock` for reproducible builds; this is distinct from the MSRV declared in `rust-version`.

**Clippy lint groups** ([Clippy lint docs](https://doc.rust-lang.org/clippy/lints.html)): `correctness` (deny), `suspicious`, `complexity`, `perf`, `style` (warn) are on by default. `pedantic` is opt-in ("cherry-pick… expect to use `#[allow]` generously"); `restriction` must be cherry-picked only — Clippy warns if you enable the whole group; `nursery` is cherry-pick only (buggy lints); `cargo` is for published crates. Large production workspaces (uv, ruff) enable `pedantic` at `priority = -2` with a curated allow-list and cherry-pick restriction lints such as `print_stdout`, `print_stderr`, `dbg_macro`, `exit`, `get_unwrap`, `rc_mutex`, `iter_over_hash_type`, plus `rust` lints `unsafe_code = "warn"`, `unreachable_pub = "warn"` ([uv Cargo.toml](https://github.com/astral-sh/uv/blob/main/Cargo.toml), [ruff Cargo.toml](https://github.com/astral-sh/ruff/blob/main/Cargo.toml)).

### 2. Workspace layout and hexagonal architecture in Rust

**Flat `crates/` layout with a virtual root manifest is the de-facto standard for large Rust projects.** matklad (rust-analyzer) argues that for 10k–1M LOC "the flat layout makes the most sense": `ls ./crates` is the architecture overview, Cargo's namespace is flat anyway, hierarchies rot, and **folder name = crate name** keeps reverse-dependency manifests unambiguous. Internal unpublished crates use `version = "0.0.0"`, and automation lives in a Rust `xtask` crate rather than shell scripts ([Large Rust Workspaces](https://matklad.github.io/2021/08/22/large-rust-workspaces.html)).

Concrete production example — **uv** has **74 crate directories** under `crates/` (73 prefixed `uv-*` plus the `uv` binary crate; corrected from 91, GitHub API 2026-10-06) (`uv-cache`, `uv-client`, `uv-resolver`, `uv-distribution-types`, `uv-fs`, `uv-cli`…), `members = ["crates/*"]` (excluding the nightly-only `uv-trampoline`), `resolver = "2"`, a `[workspace.package]` with `edition = "2024"`, `rust-version = "1.97.0"`, and every internal crate listed in `[workspace.dependencies]` with `path = "crates/…"` so members write `uv-cache = { workspace = true }` ([uv crates/](https://github.com/astral-sh/uv/tree/main/crates), [uv Cargo.toml](https://github.com/astral-sh/uv/blob/main/Cargo.toml)). ruff uses the same pattern (`ruff_*` crates) and the same lint table ([ruff Cargo.toml](https://github.com/astral-sh/ruff/blob/main/Cargo.toml)). Notable patterns worth copying: separate `*-types` crates (pure data, no IO) that many crates depend on, which keeps the compile graph wide rather than deep; a dedicated `*-fs` crate wrapping filesystem quirks; and a dedicated `*-cli`/binary crate that is the only place wiring adapters together.

**Hexagonal architecture in Rust** ([How To Code It: Master hexagonal architecture in Rust](https://www.howtocodeit.com/guides/master-hexagonal-architecture-in-rust)):
- Ports are traits owned by the domain; adapters implement them in outer crates. Typical bound set for a port used across tasks: `Clone + Send + Sync + 'static`.
- Prefer **generic services** (`struct Scanner<F: FileSystem, S: EvidenceStore>`) over `Box<dyn Port>`: monomorphised, zero-cost, still fully mockable in tests.
- Each port operation gets a **domain error enum** that fully describes what can go wrong, with an `Unknown(anyhow::Error)`-style catch-all for adapter failures.
- Start with coarse domains, split on observed friction.

**Async fn in traits (AFIT) — current status:**
- Native `async fn` / return-position `impl Trait` in traits has been stable since Rust 1.75 for **static dispatch**.
- **They are still not dyn-compatible.** The Rust Reference's dyn-compatibility rules state a dispatchable method must "Not be an `async fn` (which has a hidden `Future` type)" and "Not have a return position `impl Trait` type" ([Reference: Traits § Dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html)). A May 2026 users-forum thread confirms an RFC for async-fn-in-dyn-trait is still "in the drafting process" ([users.rust-lang.org, "Do we still need to use async_trait"](https://users.rust-lang.org/t/do-we-still-need-to-use-async-trait/140230)). Some blog posts claim dyn async traits are stable — **they are wrong** as of 2026-10.
- **`Send` bounds:** a bare `async fn` in a public trait does not promise `Send` futures (the `async_fn_in_trait` lint warns about this). Either write `fn op(&self) -> impl Future<Output = T> + Send` by hand, or use **`trait-variant` 0.1.3** (`#[trait_variant::make(Port: Send)]` generates a `Send` variant of a local trait) ([trait-variant docs](https://docs.rs/trait-variant/latest/trait_variant/)).
- **Dynamic dispatch options:** (a) **`dynosaur` 0.3.1** — `#[dynosaur::dynosaur(DynPort = dyn(box) Port)]` generates a `DynPort` wrapper; static calls stay unboxed, only dyn calls box the future; developed as part of the Rust project's async-fn-in-traits goal work ([project goals update](https://blog.rust-lang.org/2024/12/16/project-goals-nov-update/)) and recommended on the forum over async-trait ([dynosaur docs](https://docs.rs/dynosaur/latest/dynosaur/)). (b) **`async-trait` 0.1.92** — still works and is still maintained, but boxes *every* call (static and dyn) and hides the signature behind a macro ([async-trait](https://github.com/dtolnay/async-trait)). (c) Hand-written `fn op(&self) -> Pin<Box<dyn Future<Output = T> + Send + '_>>` for one or two methods.
- So: **async-trait is no longer required, but some dyn mechanism still is** if a port must be a trait object (plugin registries such as "list of `Box<dyn Collector>`" per platform).

Lumen-specific note: most Lumen ports (filesystem walker, metadata reader, quarantine mover, process inspector) are naturally **synchronous and blocking** (OS calls). Keeping those ports **sync** and calling them from a blocking pool avoids the AFIT/dyn issue entirely and is the honest model of what the OS does (see §3).

### 3. Async runtime: Tokio patterns

**Versions (verified on crates.io 2026-10-05):** `tokio` **1.53.2** (published 2026-10-03, alongside LTS backport 1.51.5); `tokio-util` **0.7.19**. Tokio LTS lines: **1.51.x until March 2027, 1.53.x until September 2027** (both MSRV 1.71); Tokio's rolling MSRV policy is "at least 6 months", and the README lists MSRV **1.85 for 1.54+** ([tokio README](https://github.com/tokio-rs/tokio), [crates.io](https://crates.io/crates/tokio)). **Pin Lumen to `tokio = "~1.53"`** (LTS) and move LTS-to-LTS deliberately.

**Structured concurrency building blocks:**

| Need | Tool | Key semantics |
|---|---|---|
| Fan-out N tasks and collect results | `tokio::task::JoinSet` | `spawn`, `spawn_blocking`, `join_next` (cancel-safe), `join_all`, `abort_all`, `shutdown`; **dropping the JoinSet aborts all tasks**; implements `FromIterator`/`Extend` ([JoinSet](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html)) |
| Track long-lived/background tasks without keeping results | `tokio_util::task::TaskTracker` | frees memory on task exit (JoinSet keeps every result → can OOM), `&self` API, does **not** abort on drop; `close()` + `wait()` returns when closed **and** empty ([TaskTracker](https://docs.rs/tokio-util/latest/tokio_util/task/task_tracker/struct.TaskTracker.html)) |
| Cooperative cancellation | `tokio_util::sync::CancellationToken` | `cancel`, `cancelled()`, `cancelled_owned()`, `is_cancelled`, `run_until_cancelled(fut)`, `drop_guard()`; **`child_token()` is cancelled by the parent but cancelling a child does not cancel the parent** ([CancellationToken](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html)) |
| Bounded concurrency | `tokio::sync::Semaphore` (`acquire_owned` → move permit into the task) | caps in-flight work (e.g. concurrent hash jobs per volume) |
| Backpressure between stages | bounded `tokio::sync::mpsc::channel(n)` | `send().await` waits when full; never use `unbounded_channel` between scanner and graph builder |

**Cancellation semantics to design around:** `spawn_blocking` work **cannot be aborted once started** — `abort()` has no effect — and **runtime shutdown waits indefinitely** for started blocking tasks (`shutdown_timeout` bounds the wait but the threads keep running) ([spawn_blocking](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)). Blocking code must therefore poll a cancellation flag itself (e.g. check `token.is_cancelled()` every directory or every N entries). Default blocking pool cap is **512 threads** (`Builder::max_blocking_threads`); excess work queues. For CPU-bound parallel work (hashing, graph analysis) the docs recommend a dedicated executor such as **rayon** rather than flooding `spawn_blocking`; for long-lived blocking loops prefer a dedicated `std::thread`.

**`tokio::fs` vs `std::fs`:** Tokio states that "most operating systems do not provide asynchronous file system APIs", so **every `tokio::fs` op is a `spawn_blocking` call** under the hood; io_uring "may" be used in the future but is not today. Guidance: batch work into as few blocking calls as possible, use `BufWriter`, build data in memory and write once, and for complex sequences **use `std::fs` inside one explicit `spawn_blocking`**. `File::set_max_buf_size` controls the per-call chunk (default 2 MB) ([tokio::fs](https://docs.rs/tokio/latest/tokio/fs/index.html)). Nuance: the `tokio::fs` module docs still say "Tokio will always use `spawn_blocking` on all platforms", but `runtime::Builder` now documents an `enable_io_uring()` method behind the `io-uring` crate feature ([Builder docs](https://docs.rs/tokio/latest/tokio/runtime/struct.Builder.html)); which `fs` ops it covers and whether it needs `--cfg tokio_unstable` is (unverified). It is Linux-only by nature and irrelevant to Lumen's macOS/Windows/iOS targets (and unreliable on Android, where io_uring is commonly restricted), so the design conclusion stands. For a scanner that does millions of `stat`/`readdir` calls, `tokio::fs::read_dir` + `metadata` per entry means one thread-pool hop per syscall — strictly worse than a synchronous walker on a blocking/rayon thread streaming batches into a bounded channel.

### 4. Error handling

**Versions (crates.io, 2026-10-05):** `thiserror` **2.0.21** (2026-09-23), `anyhow` **1.0.104**, `snafu` **0.9.2**, `error-stack` 0.8.0, `miette` 7.6.0, `color-eyre` 0.6.5 (last release 2025-05).

- **thiserror 2.x** is the current major. 2.0.0 changes vs 1.x: raw-identifier fields in format strings (`{r#type}`) no longer accepted (write `{type}`); bounds no longer inferred on fields shadowed by named format args; `{0}` cannot be mixed with extra positional args; **the using crate must depend on `thiserror` directly** (no re-export-only usage). New: `no_std` via `default-features = false`, `r#source` to opt a field out of `source()`, `#[error(fmt = path)]` out-of-line formatting, per-variant `#[error(transparent)]` ([thiserror 2.0.0 release](https://github.com/dtolnay/thiserror/releases/tag/2.0.0), [releases](https://github.com/dtolnay/thiserror/releases)). thiserror 1.x and 2.x can coexist in one dependency graph, so upgrading is incremental.
- **anyhow** remains the standard for *application* edges (CLI `main`, Tauri command glue, xtask): `Context::context()` / `with_context(|| …)` attach human context, `{:#}` prints the chain, backtraces captured when `RUST_BACKTRACE`/`RUST_LIB_BACKTRACE` is set.
- **snafu 0.9** is the main alternative: derive generates *context selectors* (`ConfigFileSnafu { path }`) so `.context(ConfigFileSnafu { path })` both wraps the source and attaches structured fields; `ensure!`, `Whatever` for prototyping, implicit `Location`/backtrace fields, `Report`/`#[snafu::report]` for chain printing ([snafu docs](https://docs.rs/snafu/latest/snafu/)). Its advantage is forcing *where* an IO error happened into the type (e.g. `ReadMetadata { path }` vs `MoveToQuarantine { path }` both wrapping `io::Error`), which thiserror's `#[from]` tends to collapse.

**Pattern for Lumen:** library crates expose **typed, non-exhaustive (`#[non_exhaustive]`) error enums with thiserror**, one per port/use-case, carrying structured fields (path, operation, OS error code) rather than formatted strings. Avoid blanket `#[from] io::Error` — a bare `io::Error` loses which file and which operation, which is exactly what a safety-critical cleanup audit log needs. Use **anyhow only in binaries**. Safety-relevant classification (e.g. `PermissionDenied`, `NotFound`-raced, `CrossDevice`) must be explicit variants because the policy engine and rollback logic branch on them.

### 5. Observability

**Versions (crates.io, 2026-10-05):** `tracing` **0.1.44**, `tracing-subscriber` **0.3.23**, `tracing-appender` 0.2.5, `tracing-error` 0.2.1; `opentelemetry` / `opentelemetry_sdk` / `opentelemetry-otlp` / `opentelemetry-appender-tracing` **0.33.0** (2026-09-18); `tracing-opentelemetry` **0.34.0** (2026-09-23); `tracing-tracy` 0.12.0; `console-subscriber` 0.5.0.

- **tracing-subscriber** composes a `Registry` with `Layer`s; **per-layer filtering** (`Filter`) lets e.g. a JSON file layer record `debug` for `lumen_*` while stderr shows `info`; `EnvFilter` gives `RUST_LOG`-style directives; the `fmt` layer has a `json` feature; a `reload` layer swaps filters at runtime (useful for a "diagnostic mode" toggle in the UI) ([tracing-subscriber docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)). `tracing-appender` provides non-blocking writers and rolling files.
- **OpenTelemetry Rust status** (from the repo README status table) ([opentelemetry-rust](https://github.com/open-telemetry/opentelemetry-rust)):

| Signal | API | SDK | OTLP exporter |
|---|---|---|---|
| Logs | Stable | Stable | RC |
| Metrics | Stable | Stable | RC |
| Traces | **Beta** | **Beta** | **Beta** |

  Also from the same table: `Logs-Appender-Tracing` **Stable**, Metrics-Prometheus exporter Beta, Context and Propagators **Beta**, Baggage RC. No 1.0 crate release yet (still 0.x); the project supports current stable plus three prior minors (floor 1.75). Recommended bridge for logs is `opentelemetry-appender-tracing` (tracing events → OTel LogRecords); the project says "If you are starting fresh, we recommend using tracing as your logging API". Spans → OTel traces still go through `tracing-opentelemetry` (separately versioned, 0.34). Expect breaking changes on every 0.x bump; the four OTel crates must move in lockstep.

**Implication:** Lumen is local-first with no telemetry upload by default, so the OTel exporter is an **optional, off-by-default adapter crate** (behind a Cargo feature and a user consent flag) — useful for the web dashboard / fleet scenario and for internal dogfooding. The always-on sink is local: `tracing` → rolling JSON files (redacted) + an in-memory ring buffer for "export diagnostics". Paths and file names are personal data; add a redaction `Layer` (hash or tokenise home-dir paths) before any sink that can leave the device.

### 6. Serialization, JSON Schema, and TypeScript types

**Versions (crates.io, 2026-10-05):** `serde` **1.0.229**, `serde_json` **1.0.151**, `schemars` **1.2.2**, `ts-rs` **12.0.1** (2026-01-31; crates.io `rust-version` 1.78 — corrected from 1.88), `specta` **1.0.5 stable / 2.0.0-rc.25** (2026-05-07; `tauri-specta` 2.0.0-rc.25), `typeshare` 1.0.5.

- **schemars 1.x** (1.0 shipped; now 1.2.2) generates **JSON Schema draft 2020-12 by default**, other drafts via `SchemaSettings`, and reads `#[serde(...)]` attributes so the schema "should match how serde_json would serialize/deserialize" ([schemars docs](https://graham.cool/schemars/)). There is a dedicated "Migrating from 0.8" guide — 1.x is a breaking rewrite (schemas are now `serde_json::Value`-backed `Schema`), so pick 1.x from the start.
- **ts-rs 12**: `#[derive(TS)] #[ts(export)]`; bindings are written when `cargo test` runs; output dir via `TS_RS_EXPORT_DIR` (default `./bindings`); `serde-compat` (default) honours `rename`, `tag`, `content`, `skip`, `flatten`; feature-gated impls for `chrono`, `uuid`, `serde_json`, `indexmap`, `url`, `bytes` ([ts-rs](https://github.com/Aleph-Alpha/ts-rs)). Mature, TS-only, generation tied to the test harness.
- **specta**: TypeScript and Swift exporters marked **Stable**; OpenAPI, Kotlin, Go, C#, Java, JSON Schema, Zod, Valibot, Python "Partially implemented… still stabilizing" ([specta](https://github.com/specta-rs/specta)). The 2.0 line has been in RC since July 2023 (rc.1 2023-07-17; rc.25 in May 2026; corrected from "since 2024") — `tauri-specta` (typed Tauri commands/events) depends on it. Pre-1.0-quality churn risk.
- **typeshare** (1Password): CLI (`typeshare ./crate --lang=typescript|swift|kotlin|scala`, Go/Python experimental) driven by `#[typeshare]` annotations ([typeshare](https://github.com/1Password/typeshare)). Multi-language but less expressive for generics/complex enums.

**Recommendation shape:** make **JSON Schema (schemars) the canonical contract** for anything that crosses a process or version boundary (evidence records, policy decisions, quarantine manifests, Jev judge input/output, dashboard sync payloads) — it is language-neutral, versionable, and testable with snapshot tests. Generate TS types for the React/Next.js/Expo clients either from Rust directly (**ts-rs** for stability, or **tauri-specta** if typed Tauri command bindings outweigh RC risk) or from the JSON Schema. Swift/Kotlin types come from UniFFI (see 01-desktop-architecture), not from these tools.

### 7. Testing and benchmarking

**Versions (crates.io, 2026-10-05):** `cargo-nextest` **0.9.146**, `proptest` **1.11.0**, `quickcheck` 1.1.0, `insta` / `cargo-insta` **1.49.0**, `tempfile` **3.27.0**, `criterion` **0.8.2**, `divan` 0.1.21 (last release 2025-04-10), `cargo-fuzz` 0.13.2, `arbitrary` 1.4.2, `bolero` 0.13.7, `loom` 0.7.2 (last release 2024-04-23), `shuttle` 0.9.5, `rstest` 0.27.0, `mockall` 0.15.0, `cargo-llvm-cov` 0.9.1, `cargo-mutants` 27.1.0.

| Area | Choice | Why / notes |
|---|---|---|
| Runner | **cargo-nextest** | process-per-test isolation, retries + flaky detection, `slow-timeout`, test groups with mutual exclusion (serialise tests that touch a shared fixture volume), setup scripts, JUnit XML, archive + partition across CI workers, leak detection, record/replay; "up to 3× faster than cargo test". **Doctests are not supported** — run `cargo test --doc` separately ([nexte.st](https://nexte.st/)). |
| Property tests | **proptest** | explicit `Strategy` values, many generators per type, range constraints, `prop_map` composition, smarter shrinking, persisted regression files; trade-off: complex value generation "up to an order of magnitude slower" than quickcheck ([proptest book: vs quickcheck](https://altsysrq.github.io/proptest-book/proptest/vs-quickcheck.html)). Use it for policy invariants (e.g. "no path under a protected root is ever QUARANTINE"). |
| Snapshots | **insta** | `assert_snapshot!`, `assert_debug_snapshot!`, `assert_json_snapshot!`, `assert_yaml_snapshot!`, redactions (mask timestamps, UUIDs, home dirs), inline snapshots, `glob!`, `cargo insta review`; honours `INSTA_UPDATE`/`CI`; documented nextest compatibility ([insta docs](https://insta.rs/docs/)). Ideal for evidence-graph and policy-decision explanations, and for JSON Schema files. |
| Temp FS fixtures | **tempfile** | `TempDir` auto-cleans on drop; build realistic fake app/cache trees per test. |
| Benchmarks | **criterion 0.8** (primary), divan (optional) | criterion is now maintained by the `criterion-rs` org and supports the last three stable Rust minors ([criterion.rs](https://github.com/criterion-rs/criterion.rs)); statistically rigorous, HTML reports, baseline comparison for CI regression gates. divan has nicer ergonomics (`#[divan::bench]`, allocation counting) but no release since 2025-04 ([divan](https://github.com/nvzqz/divan)) — maintenance risk. |
| Fuzzing | **cargo-fuzz** (+ `arbitrary`) | libFuzzer only; "requires the nightly compiler since it uses the `-Z` compiler flag to provide address sanitization"; works on x86-64 Linux, x86-64 and Apple-Silicon macOS, and Windows ([Rust Fuzz Book: setup](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html), [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html)). Fuzz parsers of untrusted on-disk formats (plists, `.lnk`, registry exports, app manifests, quarantine manifests). `bolero` can run the same harness as a proptest-style test on stable. |
| Concurrency model checking | **loom** for small lock-free/atomic primitives; consider **shuttle** (randomised, scales to larger tests) | loom is exhaustive but slow and has had no release since 2024-04 (still the reference tool). Only needed if Lumen writes its own sync primitives; prefer to not write any. |

Also useful: `cargo-llvm-cov` for coverage, `cargo-mutants` for mutation testing of the policy engine (where a surviving mutant = an untested safety rule).

### 8. Supply chain

**Versions (crates.io, 2026-10-05):** `cargo-deny` **0.20.2**, `cargo-audit` **0.22.2**, `cargo-vet` **0.10.2**, `cargo-auditable` 0.7.7, `cargo-machete` 0.9.2, `cargo-udeps` 0.1.61, `cargo-semver-checks` 0.51.0, `cargo-hack` 0.6.45.

- **RustSec** advisory DB (Rust Secure Code WG) feeds `cargo-audit`, `cargo-deny`, OSV, the GitHub Advisory Database (Dependabot) and Debian's tracker. `cargo-auditable` embeds the dependency list into shipped binaries so a released Lumen build can be re-audited later (`cargo audit bin`) ([rustsec.org](https://rustsec.org/)).
- **cargo-deny** runs four checks: **advisories** (vulnerable, unmaintained, yanked; v2 config format), **bans** (deny specific crates, flag duplicate versions), **licenses** (allow-list SPDX), **sources** (only crates.io / approved git) — plus `cargo-deny-action` for CI ([cargo-deny checks](https://embarkstudios.github.io/cargo-deny/checks/index.html)). It subsumes `cargo-audit` for CI gating.
- **cargo-vet**: records that each third-party crate version was audited by you or by an imported trusted party; supports relative (delta) audits and exemptions for incremental adoption ([cargo-vet book](https://mozilla.github.io/cargo-vet/)). Built-in criteria: **safe-to-run** (no surprising effects when built/run/tested locally) and **safe-to-deploy** (no serious vulnerability when exposed to untrusted input; requires reviewing `unsafe` and powerful imports) — safe-to-deploy implies safe-to-run ([built-in criteria](https://mozilla.github.io/cargo-vet/built-in-criteria.html)).
- **Unused deps:** `cargo-machete` is "fast (yet imprecise)" (text search of sources, works on stable, configurable ignores) ([cargo-machete](https://github.com/bnjbvr/cargo-machete)); `cargo-udeps` is precise but compiles the workspace and "needs Rust nightly to actually run" (`cargo +nightly udeps`) ([cargo-udeps](https://github.com/est31/cargo-udeps)). Run machete on every PR, udeps weekly.
- The Rust blog's 2026-09 notices about **targeted attacks on prominent Rustaceans** (2026-09-17) and **GitHub Actions leaking secrets via cached Miri output** (2026-09-21) argue for: pinned action SHAs, no secrets in jobs that build untrusted PRs, `cargo-vet` for new deps, and minimal-permission CI tokens ([Rust blog](https://blog.rust-lang.org/)). Precise mechanism of the Miri issue: `cargo miri` stored **all environment variables** under `target/`; a job on `main` (which *has* secrets and *writes* the cache) caches `target/`, and PR jobs (which only *read* the cache) can then read those secrets. So "no secrets on PR builds" alone does **not** fix it — the fix is to never cache `target/` from jobs that have secrets in their environment, and to use a Miri from nightly **2026-09-22 or later**, which only preserves `CARGO_*` (excluding `CARGO_*_TOKEN`) and `OUT_DIR` ([Miri secrets advisory](https://blog.rust-lang.org/2026/09/21/github-actions-leaking-secrets-when-miri-output-is-cached/)).
- **Omitted earlier: a real crates.io supply-chain compromise on 2026-08-20.** `arrayref@0.3.10`, `internment@0.8.7` and `append-only-vec@0.1.9` were republished from a likely-compromised maintainer account with new dependencies on malicious crates (e.g. `proc-macro1`) whose **build script downloaded a payload**; they were live for 86–107 minutes ([Supply chain attack on arrayref](https://blog.rust-lang.org/2026/08/20/supply-chain-attack-on-arrayref/)). `arrayref` is a common transitive dependency (unverified whether it is in Lumen's planned graph). Implications: commit `Cargo.lock`, use `cargo build --locked`/`--frozen` in CI and release builds, review lockfile diffs that add new crates (cargo-vet makes this a gate), and treat build scripts/proc-macros as code execution on developer and CI machines.
- Cargo itself had security advisories in 2026 (CVE-2026-33056 on 2026-03-21; CVE-2026-5222 and CVE-2026-5223 on 2026-05-25, fixed in Rust 1.96.0). The May pair only affect **third-party registries** (crates.io forbids symlinks in uploads), so they matter to Lumen only if a private registry is adopted; the pinned 1.99.0 toolchain includes the fixes ([CVE-2026-5223](https://blog.rust-lang.org/2026/05/25/cve-2026-5223/), [CVE-2026-5222](https://blog.rust-lang.org/2026/05/25/cve-2026-5222/), [CVE-2026-33056](https://blog.rust-lang.org/2026/03/21/cve-2026-33056/)). The details of CVE-2026-33056 are (unverified).

### 9. Persistence

**Versions (crates.io, 2026-10-05):** `rusqlite` **0.40.2** (bundled `libsqlite3-sys` 0.38.2; README states **SQLite 3.53.2** for 0.40.1), `sqlx` **0.9.0** (published to crates.io 2026-05-21; MSRV 1.94), `diesel` **2.3.13** (MSRV 1.86), `sea-orm` **2.0.4** (2026-09-27, MSRV 1.94; 2.0 stable announced 2026-07-27), `rusqlite_migration` **2.6.0** (2026-05-28, **MSRV 1.95** — this sets the floor for Lumen's workspace `rust-version`; 2.5.0 is the last release with MSRV 1.84), `refinery` 0.10.0, `tokio-rusqlite` 0.8.0, `r2d2_sqlite` 0.35.0, `deadpool-sqlite` 0.14.0. Latest upstream SQLite: **3.53.4** (2026-07-24) ([SQLite changes](https://www.sqlite.org/changes.html)).

| Option | Model | Fit for Lumen's embedded local store |
|---|---|---|
| **rusqlite** | sync, thin, full SQLite feature surface | **Best fit.** `bundled` pins the SQLite version on every OS; features `backup` (online backup API), `blob` (incremental BLOB IO), `hooks` (commit/rollback/update hooks), `functions` (Rust UDFs), `vtab`, `array` (`rarray()`), `serde_json`, `jiff`/`chrono`, `uuid`, `bundled-sqlcipher` (encryption at rest); `prepare_cached` statement cache; MSRV = "Latest stable Rust version at the time of release. It might compile with older versions." ([rusqlite](https://github.com/rusqlite/rusqlite)). **Caveat:** rusqlite 0.40.2 (2026-08-08, when stable was 1.97.x) declares **no `rust-version`** on crates.io, so resolver 3's MSRV-aware fallback cannot protect a lower workspace MSRV; whether it builds on 1.95 is (unverified) and must be proven by the MSRV CI job. No async — run on a dedicated DB thread / blocking pool. |
| **sqlx 0.9** | async, compile-time-checked SQL macros | Good for server-side Postgres (dashboard backend). For SQLite it still runs SQLite on worker threads (SQLite has no async IO), compile-time checks need `DATABASE_URL` or offline `.sqlx` data. 0.9 adds `sqlx.toml` per-crate config, `SqlSafeStr` (queries must be `&'static str` or `AssertSqlSafe(...)`), significant `Migrate` trait changes, repo moving to `transact-rs` org, and `Cargo.lock` no longer tracked upstream ([sqlx CHANGELOG](https://github.com/launchbadge/sqlx/blob/main/CHANGELOG.md)). |
| **diesel 2.3** | sync ORM/query builder, compile-time typed schema | Strong typing, `diesel_migrations` 2.3.2 `embed_migrations!`; heavier compile times and DSL learning curve; SQLite supported. |
| **SeaORM 2.0** | async ORM on sqlx; new synchronous variant ("Synchronous SeaORM", `sea-orm-sync`; rusqlite backing is (unverified) — the 2.0 announcement does not name the driver) | Just went stable after 43 RCs; MSRV 1.94; new entity format; adds Arrow/Parquet export ([Announcing SeaORM 2.0](https://www.sea-ql.org/blog/2026-07-27-sea-orm-2.0/)). Too much abstraction and churn for a safety-critical audit store. |

**SQLite configuration facts:**
- **WAL**: readers don't block the writer and vice versa, but **one writer at a time**; auto-checkpoint at 1000 pages by default; `synchronous=NORMAL` in WAL skips fsync on commit (fsync at checkpoint) — fast, but the last transactions can roll back after power loss; WAL mode is **persistent** in the file; **not for network filesystems**; `-wal` and `-shm` files must travel with the DB (never copy just `db` file; use the backup API or `VACUUM INTO`) ([SQLite WAL](https://www.sqlite.org/wal.html)).
- **WAL-reset corruption bug**: per the SQLite WAL page it is "likely present in all version of SQLite from 3.7.0 (2010-07-21) through 3.51.2" and "fixed in version 3.51.3 (2026-03-13) and later" (so every 3.53.x includes it), with backports in **3.44.6 and 3.50.7** ([SQLite WAL § WAL-reset bug](https://www.sqlite.org/wal.html), [SQLite changes](https://www.sqlite.org/changes.html)). Trigger conditions: WAL mode **and two or more connections on the same file, in separate threads or processes, writing or checkpointing at the same instant**. SQLite calls it rare ("unlikely to occur in common use") but serious. For Lumen this argues for (a) `rusqlite` `bundled` rather than OS-provided libsqlite3 (system SQLite on older macOS/iOS/Windows/Android builds may predate the fix) and (b) the single-writer-connection design in R3/R9, which also keeps Lumen away from the trigger pattern. Note 3.52.0 (2026-03-06) was **withdrawn**; never pin it.
- **Bundled version lags upstream:** `libsqlite3-sys` 0.38.2 (latest, 2026-08-08) bundles **3.53.2** (verified from `sqlite3.h` in the published crate), while upstream 3.53.3 (2026-06-26) and 3.53.4 (2026-07-24) ship "fixes for problems in 3.53.0 (and 3.53.1, 3.53.2…) mostly coming from AIs". Track `libsqlite3-sys` releases and bump when it picks up 3.53.4+.
- **STRICT tables** (SQLite ≥ 3.37.0): column types restricted to `INT, INTEGER, REAL, TEXT, BLOB, ANY`; values that cannot be losslessly coerced raise `SQLITE_CONSTRAINT_DATATYPE`; combinable with `WITHOUT ROWID`; same file format ([STRICT tables](https://www.sqlite.org/stricttables.html)).
- **Migrations**: `rusqlite_migration` 2.6 tracks schema version in `PRAGMA user_version` (an integer in the file header, no table), `Migrations::new(vec![M::up(...)])`, `to_latest()` applies atomically, `validate()` for a unit test, optional down migrations, foreign-key check after migrating, `from-directory` feature to load `*.sql` ([rusqlite_migration docs](https://docs.rs/rusqlite_migration/latest/rusqlite_migration/)). `refinery` is the alternative with a history table and multi-DB support.

## Implications for Lumen

### R1. Workspace skeleton (adopt now)

```
lumen/
  Cargo.toml            # virtual manifest, resolver = "3"
  rust-toolchain.toml   # channel = "1.99.0", components = ["clippy","rustfmt"], targets = [...]
  deny.toml  supply-chain/ (cargo-vet)  .config/nextest.toml
  crates/
    lumen-types/          # pure data: ids, paths, evidence, decisions (serde + schemars), no IO
    lumen-policy/         # deterministic KEEP/REVIEW/QUARANTINE engine; depends only on lumen-types
    lumen-graph/          # evidence graph model + queries
    lumen-core/           # use-cases (scan, plan, quarantine, verify, rollback); defines ports as traits
    lumen-ports-testkit/  # in-memory fakes of every port + proptest strategies
    lumen-fs/             # FS adapter: walker, metadata, safe move/rename, set_times_nofollow
    lumen-store-sqlite/   # rusqlite adapter (evidence, plans, quarantine ledger, audit log)
    lumen-platform-macos/ lumen-platform-windows/ lumen-platform-android/ lumen-platform-ios/
    lumen-jev/            # optional AI judge adapter: evidence-only, feature-gated
    lumen-telemetry/      # tracing setup, redaction layer, optional OTel exporter (feature "otel")
    lumen-ffi/            # UniFFI facade
    lumen-app/            # composition root: wires adapters into core (Tauri host / agent / CLI)
  xtask/                # schema export, TS type export, release checks
```

Rationale: this is the flat layout that rust-analyzer, uv and ruff use. Making `lumen-types` and `lumen-policy` IO-free lets the safety-critical policy compile fast, test exhaustively, and be reused unchanged on every platform and in the dashboard backend. Platform adapters are crates selected with `[target.'cfg(target_os = "...")'.dependencies]` in `lumen-app`, so the core never contains `#[cfg(target_os)]` sprawl. **Rejected:** a nested hierarchy (`crates/platform/macos/...`), which goes stale (matklad). Also rejected: a single crate with modules, which brings slow incremental builds, no enforced dependency direction, and no compile-time proof that the policy crate cannot touch the filesystem.

Root manifest essentials:

```toml
[workspace]
members = ["crates/*", "xtask"]
resolver = "3"                      # mandatory: virtual manifest has no edition

[workspace.package]
edition = "2024"
rust-version = "1.95"               # MSRV floor set by rusqlite_migration 2.6 (1.95); rusqlite 0.40 declares none (latest-stable policy) — verify in CI; toolchain pinned separately to 1.99.0
version = "0.0.0"
publish = false

[workspace.dependencies]
lumen-types = { path = "crates/lumen-types" }
tokio = { version = "~1.53", default-features = false }   # LTS line
thiserror = "2.0.21"
rusqlite = { version = "0.40", features = ["bundled"] }

[workspace.lints.rust]
unsafe_code = "deny"                # allow per-module in platform crates with SAFETY comments
unreachable_pub = "warn"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
module_name_repetitions = "allow"
missing_errors_doc = "allow"
unwrap_used = "warn"                # restriction, cherry-picked
expect_used = "warn"
panic = "warn"
print_stdout = "warn"
print_stderr = "warn"
dbg_macro = "warn"
exit = "warn"
indexing_slicing = "warn"           # for lumen-policy / lumen-core only, via crate-level attrs if noisy
```

Choose the MSRV deliberately. Setting `rust-version` to 1.99 gives the edition-2024 `default-features` override behavior for workspace dependencies. A lower MSRV keeps room for Tauri's "stable − 3" policy. Either way, CI must test the MSRV with `cargo hack --rust-version` or a dedicated job.

### R2. Ports: sync and generic by default, async only at IO edges

- Model filesystem, metadata, process and launch-agent inspection, and quarantine moves as **synchronous traits** (`trait FileSystem: Send + Sync`). Run them on a blocking or rayon pool, driven by a Tokio orchestration layer. This matches the OS reality (`tokio::fs` is a thread-pool hop anyway) and sidesteps AFIT dyn limitations.
- Use **generic services** (`ScanService<F: FileSystem, S: Store>`) for performance and testability. Where a heterogeneous registry is needed (e.g. `Vec<Box<dyn Collector>>` of per-platform collectors), use sync dyn traits, or `dynosaur` for the few async ones. **Rejected:** `async-trait` everywhere, because it boxes every call and hides signatures. It stays acceptable for an isolated network adapter (Jev, dashboard sync).
- Async ports (`#[trait_variant::make(Send)]` or explicit `-> impl Future + Send`) only for network-bound adapters: Jev client, dashboard sync, update checks.

### R3. Concurrency and cancellation contract

- One `CancellationToken` per user operation (scan, cleanup), with `child_token()` per volume or sub-task. Cancelling a sub-task never cancels the operation. A `TaskTracker` owns long-lived agent tasks, and `JoinSet` is used inside a single operation's fan-out.
- **Blocking work must check the token cooperatively.** Pass a cheap `Arc<AtomicBool>`, or `token.clone()` and `is_cancelled()`, into the walker, and check it per directory. Destructive steps (quarantine move) must be **uninterruptible units with a journal entry before and after**, so cancellation can only happen *between* items, never mid-move.
- Use bounded channels between walker → evidence collector → graph builder → store writer (e.g. capacity 1–4k entries) and a `Semaphore` for hashing and deep inspection. Run a single **store-writer task/thread** that batches inserts into one transaction per N rows, which is the right shape for SQLite's single-writer model.
- Configure the Tokio runtime explicitly, with `max_blocking_threads` set to a modest number (e.g. 32–64) and rayon sized to the cores minus one. This avoids a 512-thread blocking pool hammering a spinning disk or a phone's flash.

### R4. Errors

thiserror 2 typed errors per crate, `#[non_exhaustive]`, structured fields, and no blanket `#[from] io::Error`. Use anyhow only in `lumen-app` and `xtask`. The quarantine and rollback paths log the full error chain into the audit log. **Rejected:** snafu as the default. It is fine technically, but thiserror is more familiar to contributors and the per-call-site discipline can be enforced by review and lints. Revisit if error-context bugs show up.

### R5. Observability

`tracing` everywhere with `#[instrument(skip_all, fields(op_id, item_id))]`. Never log raw paths at `info` or above without the redaction layer. Sinks are local rolling JSON plus an in-memory ring buffer for user-exported diagnostics. OTel goes behind the `otel` feature and an explicit opt-in. Pin all `opentelemetry*` crates and `tracing-opentelemetry` together and expect breaking bumps. Traces are still Beta.

### R6. Contracts and types

`schemars` 1.x JSON Schemas for every persisted or transmitted structure, versioned (`schema_version` field), exported by `xtask` and snapshot-tested with insta so a schema change shows up as a reviewable diff. Generate TypeScript with **ts-rs 12** by default. Adopt **tauri-specta** only if typed Tauri command bindings prove worth the RC churn, and pin the exact RC if so. **Rejected:** typeshare as the primary tool (weaker expressiveness, and Swift/Kotlin are already covered by UniFFI) and hand-written TS types (drift).

### R7. Testing and CI gates

- `cargo nextest run --workspace` + `cargo test --doc`. Use nextest test groups to serialize tests that use real-volume fixtures.
- Policy engine: proptest invariants (protected roots never QUARANTINE, REVIEW never auto-executes, decision is deterministic for identical evidence), insta snapshots of decision explanations, and cargo-mutants on `lumen-policy` in a nightly job.
- Quarantine/rollback: tempfile-based integration tests that inject failures through a fault-injecting `FileSystem` port (fail on the k-th op), asserting the journal always restores the original state.
- Fuzz (cargo-fuzz, nightly job) every parser of untrusted on-disk data.
- Bench (criterion) the walker and graph builder, with a regression gate against a stored baseline.

### R8. Supply chain

`cargo deny check` (advisories, bans with `multiple-versions = "warn"`, licenses allow-list, sources = crates.io only) on every PR. `cargo vet` with imports from established audit sets, requiring `safe-to-deploy` for runtime deps and `safe-to-run` for dev/build deps. `cargo auditable build` for release artifacts. `cargo machete` per PR. GitHub Actions pinned by SHA, with no secrets on PR builds, and **no caching of `target/` (or Miri output) from any job that has secrets in its environment**, since PR jobs can read caches written by `main`; use Miri from nightly ≥ 2026-09-22. Build with `--locked` everywhere and treat any new crate in a `Cargo.lock` diff as a review item (arrayref compromise, 2026-08-20).

### R9. Persistence

`rusqlite` + `bundled` (consider `bundled-sqlcipher` only if at-rest encryption of the evidence DB is required, and weigh key-management cost). Pragmas on open: `journal_mode=WAL`, `foreign_keys=ON`, `busy_timeout`, `synchronous=FULL` **for the quarantine ledger / audit DB** (durability over speed, because a lost ledger row means an unrestorable file) and `NORMAL` for the rebuildable scan cache. Consider splitting these into **two database files** so they can have different durability settings. Keep **one writer connection per DB file** (no second process such as a helper agent writing or checkpointing the same file concurrently), which both matches SQLite's single-writer model and avoids the WAL-reset bug's trigger pattern on any unpatched system SQLite. Bump `libsqlite3-sys` when it bundles ≥ 3.53.4. All tables `STRICT`. Migrations via `rusqlite_migration` with `validate()` in tests, plus forward-only migrations in production (no down migrations on user data). Use the backup API or `VACUUM INTO` for exports and never copy the DB file alone. **Rejected:** sqlx for the local store (async adds nothing for SQLite, compile-time checks need DB setup in CI, and 0.9 churn), Diesel (compile-time cost, DSL), and SeaORM 2.0 (abstraction and fresh 2.0). sqlx 0.9 stays a candidate for the **web dashboard backend** on Postgres.

## Risks and open questions

- **MSRV vs. Tauri vs. mobile toolchains:** Tauri 2.12 MSRV is 1.90 (see 01-desktop-architecture). sqlx 0.9 needs 1.94, and the `default-features` override needs 1.99. Pick a workspace MSRV and confirm Android NDK and iOS targets build on the pinned toolchain.
- **Async-in-dyn may stabilize** in 2027+. Designs that rely on dynosaur should keep the trait shape close to native so migration is mechanical.
- **specta/tauri-specta 2.0 still RC** after more than three years (specta rc.1 2023-07-17, tauri-specta rc.1 2023-10-04). If adopted, an upstream stall could leave Lumen pinned to an old Tauri.
- **OpenTelemetry Rust traces Beta**, so 0.x breaking changes roughly every few months. Keep it isolated in `lumen-telemetry`.
- **divan and loom release staleness** (2025-04 and 2024-04). Avoid making them load-bearing.
- **SQLite on network/cloud-synced folders:** WAL is unsafe on network filesystems, so the DB must live in an app-local, non-synced directory (not iCloud Drive or OneDrive-redirected Documents). Verify per-platform default paths.
- **System SQLite vs bundled on iOS/Android:** bundling adds about 1–2 MB (unverified) and duplicates the OS library. Confirm App Store and Play policy has no issue (believed fine) and measure binary size.
- **`synchronous=FULL` cost** on mobile flash for the ledger DB needs benchmarking. The ledger write rate is low, so it is expected to be acceptable (unverified).
- **Version churn:** all version numbers here are from crates.io on 2026-10-05. For `tracing-subscriber` 0.3.23, the crates.io API gives a publish date of **2026-03-13** (re-checked 2026-10-06); docs.rs build dates can differ from publish dates. Re-verify at project bootstrap.
- **Open:** whether to require `cargo-vet` from day one (audit backlog cost) or start with exemptions and ratchet. Also open: whether `unsafe_code = "deny"` is workable in platform crates that need FFI (likely `allow` per crate with `undocumented_unsafe_blocks` as a clippy warning).

## Sources

- [crates.io](https://crates.io/) — API queried 2026-10-05 for every crate version cited (e.g. [tokio](https://crates.io/crates/tokio), [thiserror](https://crates.io/crates/thiserror), [rusqlite](https://crates.io/crates/rusqlite), [sqlx](https://crates.io/crates/sqlx), [specta](https://crates.io/crates/specta))
- [Rust Blog index](https://blog.rust-lang.org/) — release list and 2026-09/10 security notices
- [Announcing Rust 1.99.0](https://blog.rust-lang.org/2026/10/01/Rust-1.99.0/)
- [Rust project goals, Nov 2024 update](https://blog.rust-lang.org/2024/12/16/project-goals-nov-update/)
- [Edition Guide: Rust-version aware resolver](https://doc.rust-lang.org/edition-guide/rust-2024/cargo-resolver.html)
- [Cargo Book: Dependency Resolution](https://doc.rust-lang.org/cargo/reference/resolver.html)
- [Cargo Book: Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [rustup: Overrides / rust-toolchain.toml](https://rust-lang.github.io/rustup/overrides.html)
- [Clippy: Lint groups](https://doc.rust-lang.org/clippy/lints.html)
- [Rust Reference: Traits (dyn compatibility)](https://doc.rust-lang.org/reference/items/traits.html)
- [users.rust-lang.org: Do we still need to use async_trait (May 2026)](https://users.rust-lang.org/t/do-we-still-need-to-use-async-trait/140230)
- [async-trait (GitHub)](https://github.com/dtolnay/async-trait)
- [dynosaur docs](https://docs.rs/dynosaur/latest/dynosaur/)
- [trait-variant docs](https://docs.rs/trait-variant/latest/trait_variant/)
- [matklad: Large Rust Workspaces](https://matklad.github.io/2021/08/22/large-rust-workspaces.html)
- [uv crates directory](https://github.com/astral-sh/uv/tree/main/crates) and [uv Cargo.toml](https://github.com/astral-sh/uv/blob/main/Cargo.toml)
- [ruff Cargo.toml](https://github.com/astral-sh/ruff/blob/main/Cargo.toml)
- [kage1020/Cairn#335: resolver 2 pin resolves past MSRV](https://github.com/kage1020/Cairn/issues/335)
- [How To Code It: Master hexagonal architecture in Rust](https://www.howtocodeit.com/guides/master-hexagonal-architecture-in-rust)
- [Tokio README (LTS, MSRV)](https://github.com/tokio-rs/tokio)
- [tokio::fs docs](https://docs.rs/tokio/latest/tokio/fs/index.html)
- [tokio::task::spawn_blocking docs](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)
- [tokio::task::JoinSet docs](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html)
- [tokio_util TaskTracker docs](https://docs.rs/tokio-util/latest/tokio_util/task/task_tracker/struct.TaskTracker.html)
- [tokio_util CancellationToken docs](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html)
- [thiserror releases](https://github.com/dtolnay/thiserror/releases) and [thiserror 2.0.0 release notes](https://github.com/dtolnay/thiserror/releases/tag/2.0.0)
- [snafu docs](https://docs.rs/snafu/latest/snafu/)
- [tracing-subscriber docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)
- [opentelemetry-rust README (status table)](https://github.com/open-telemetry/opentelemetry-rust)
- [Schemars documentation](https://graham.cool/schemars/)
- [ts-rs (GitHub)](https://github.com/Aleph-Alpha/ts-rs)
- [specta (GitHub)](https://github.com/specta-rs/specta)
- [typeshare (GitHub)](https://github.com/1Password/typeshare)
- [cargo-nextest](https://nexte.st/)
- [Proptest book: Differences between QuickCheck and Proptest](https://altsysrq.github.io/proptest-book/proptest/vs-quickcheck.html)
- [insta documentation](https://insta.rs/docs/)
- [criterion.rs (criterion-rs org)](https://github.com/criterion-rs/criterion.rs)
- [divan (GitHub)](https://github.com/nvzqz/divan)
- [Rust Fuzz Book: cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html) and [setup](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html)
- [cargo-deny: checks](https://embarkstudios.github.io/cargo-deny/checks/index.html)
- [cargo-vet book](https://mozilla.github.io/cargo-vet/) and [built-in criteria](https://mozilla.github.io/cargo-vet/built-in-criteria.html)
- [RustSec](https://rustsec.org/)
- [cargo-machete](https://github.com/bnjbvr/cargo-machete)
- [cargo-udeps](https://github.com/est31/cargo-udeps)
- [rusqlite (GitHub)](https://github.com/rusqlite/rusqlite)
- [sqlx CHANGELOG](https://github.com/launchbadge/sqlx/blob/main/CHANGELOG.md)
- [Announcing SeaORM 2.0](https://www.sea-ql.org/blog/2026-07-27-sea-orm-2.0/)
- [rusqlite_migration docs](https://docs.rs/rusqlite_migration/latest/rusqlite_migration/)
- [SQLite: Write-Ahead Logging](https://www.sqlite.org/wal.html)
- [SQLite: STRICT Tables](https://www.sqlite.org/stricttables.html)
- [SQLite: Release History](https://www.sqlite.org/changes.html)
- [Rust blog: Supply chain attack on arrayref (2026-08-20)](https://blog.rust-lang.org/2026/08/20/supply-chain-attack-on-arrayref/)
- [Rust blog: GitHub Actions leaking secrets when Miri output is cached (2026-09-21)](https://blog.rust-lang.org/2026/09/21/github-actions-leaking-secrets-when-miri-output-is-cached/)
- [Rust blog: Security Advisory for Cargo (CVE-2026-5223)](https://blog.rust-lang.org/2026/05/25/cve-2026-5223/), [CVE-2026-5222](https://blog.rust-lang.org/2026/05/25/cve-2026-5222/), [CVE-2026-33056](https://blog.rust-lang.org/2026/03/21/cve-2026-33056/)
- [tokio runtime::Builder docs (max_blocking_threads, enable_io_uring)](https://docs.rs/tokio/latest/tokio/runtime/struct.Builder.html)
- crates.io API (`https://crates.io/api/v1/crates/<name>`) re-queried 2026-10-06 for versions, publish dates and declared `rust-version`; `libsqlite3-sys` 0.38.2 crate tarball inspected for the bundled `SQLITE_VERSION`
- GitHub API listing of [uv crates/](https://github.com/astral-sh/uv/tree/main/crates) (2026-10-06) for the crate count

## Verification log

Fact-check pass on 2026-10-06 (adversarial review). Verdicts: **confirmed**, **corrected** (fixed in place above), **unverified** (marked in text).

| # | Claim | Verdict | Source |
|---|---|---|---|
| 1 | Stable Rust 1.99.0 released 2026-10-01; stabilizes `fs::set_times`/`set_times_nofollow`, `VaList`, `Vec::into_parts`, `String::from_utf8_lossy_owned`, `Layout::for_value_raw` | confirmed | Rust blog 1.99.0 announcement |
| 2 | Release cadence list 1.94–1.99 | corrected (patch releases 1.94.1, 1.96.1, 1.97.1 were missing) | Rust blog index |
| 3 | Rust blog security notices 2026-09-17 (targeted attacks) and 2026-09-21 (Miri secrets); i686 Windows demotion 2026-10-02 | confirmed | Rust blog index |
| 4 | Miri issue mitigated by "no secrets on PR builds" | corrected — leak path is `main`-written `target/` cache read by PRs; fix is no `target/` caching in secret-bearing jobs + Miri nightly ≥ 2026-09-22 | Miri secrets advisory |
| 5 | Supply-chain incidents relevant to CI | added (omission) — arrayref/internment/append-only-vec compromise 2026-08-20; Cargo CVE-2026-5222/5223 (third-party registries only, fixed 1.96.0) | Rust blog posts |
| 6 | Virtual workspace must set `resolver` explicitly; resolver 3 = edition 2024 default, Rust 1.84+, `fallback` is a preference; heuristics with mixed member MSRVs | confirmed (wording "silently … with a warning" fixed) | Cargo Workspaces + Resolver reference |
| 7 | Member `default-features` overrides workspace entry only for edition-2024 packages on Rust 1.99+; `optional` not allowed in `[workspace.dependencies]`; `features` additive | confirmed | Cargo Workspaces reference |
| 8 | uv: 91 `uv-*` crates, `resolver = "2"`, edition 2024, rust-version 1.97.0, pedantic priority -2 | corrected (74 crate dirs: 73 `uv-*` + `uv`); rest confirmed | GitHub API, uv Cargo.toml |
| 9 | Third-party issue kage1020/Cairn#335 exists | confirmed (exists; its "refuses to pick" wording overstates `fallback` — the doc's own preference-not-guarantee text is correct) | GitHub |
| 10 | async fn / RPIT not dyn-compatible; May 2026 forum thread says RFC in drafting and recommends dynosaur | confirmed | Rust Reference; users.rust-lang.org thread (2026-05-23) |
| 11 | Tokio 1.53.2 latest; LTS 1.51.x to Mar 2027, 1.53.x to Sep 2027 (MSRV 1.71); 1.54+ MSRV 1.85; 1.51.5 published 2026-10-03 | confirmed | Tokio README; crates.io API |
| 12 | `spawn_blocking` cannot be aborted; shutdown waits indefinitely; `shutdown_timeout` does not cancel; default `max_blocking_threads` 512; rayon suggested for CPU-bound | confirmed | tokio docs (spawn_blocking, Builder) |
| 13 | `tokio::fs` always uses `spawn_blocking`, "no io_uring" | corrected (nuance) — fs docs still say always spawn_blocking, but `Builder::enable_io_uring()` exists behind `io-uring` feature; its scope/unstable gating is (unverified); not relevant to Lumen targets | tokio::fs + Builder docs |
| 14 | thiserror 2.0.0 breaking changes and new features; 2.0.21 latest (2026-09-23, MSRV 1.77) | confirmed | thiserror 2.0.0 release notes; crates.io API |
| 15 | OpenTelemetry Rust status: logs/metrics API+SDK Stable, OTLP RC; traces Beta; MSRV 1.75 + current-3 policy; 0.33.0 on 2026-09-18; tracing-opentelemetry 0.34.0 | confirmed (added: appender-tracing Stable, Context/Propagators Beta) | opentelemetry-rust README; crates.io API |
| 16 | ts-rs 12.0.1 MSRV 1.88 | corrected → crates.io `rust-version` 1.78 | crates.io API |
| 17 | specta/tauri-specta 2.0 "RC since 2024", "about two years" | corrected → RC since 2023-07-17 (specta) / 2023-10-04 (tauri-specta), > 3 years; rc.25 May 2026 confirmed; TS + Swift exporters Stable confirmed | crates.io API; specta README |
| 18 | rusqlite 0.40.2 bundles SQLite 3.53.2; MSRV = latest stable at release | confirmed (from `sqlite3.h` in libsqlite3-sys 0.38.2); added: rusqlite declares no `rust-version`, so a 1.95 MSRV is (unverified) for it | rusqlite README; crate tarball; crates.io API |
| 19 | WAL-reset bug fixed in 3.53.0 and backported to 3.51.3 | corrected framing — fixed in 3.51.3 and all later releases, backports 3.44.6/3.50.7; affects 3.7.0–3.51.2; triggers only with ≥2 connections writing/checkpointing simultaneously; 3.52.0 withdrawn; upstream 3.53.4 (2026-07-24) confirmed | SQLite WAL page; SQLite changes |
| 20 | `synchronous=NORMAL` in WAL loses durability on power loss; WAL unsupported on network FS | confirmed | SQLite WAL page |
| 21 | sqlx 0.9.0 changelog date 2026-05-06, MSRV 1.94 | corrected date → crates.io publish 2026-05-21; MSRV confirmed | crates.io API |
| 22 | SeaORM 2.0 stable 2026-07-27 after 43 RCs, Arrow/Parquet; `sea-orm-sync` backed by rusqlite | confirmed except rusqlite backing → (unverified) | SeaORM 2.0 announcement |
| 23 | rusqlite_migration 2.6.0 | confirmed; added: MSRV 1.95 (sets workspace floor) | crates.io API |
| 24 | cargo-vet: safe-to-deploy implies safe-to-run; definitions | confirmed | cargo-vet built-in criteria |
| 25 | Clippy: do not enable whole `restriction` group | confirmed | Clippy lint docs |
| 26 | criterion maintained under criterion-rs org; supports last three stable minors | confirmed | criterion-rs/criterion.rs README |
| 27 | Remaining crate versions (tokio-util, anyhow, snafu, tracing, tracing-subscriber, serde/serde_json, schemars, nextest, proptest, insta, tempfile, divan 2025-04-10, loom 2024-04-23, cargo-fuzz, cargo-deny/audit/vet/auditable/machete/udeps, diesel, async-trait, dynosaur, trait-variant, typeshare, cargo-semver-checks, cargo-hack, cargo-mutants) | confirmed (tracing-subscriber 0.3.23 publish date 2026-03-13) | crates.io API |
| 28 | All URLs in this document resolve | confirmed (crates.io pages return 404 to non-browser clients but 200 to browsers) | HTTP check 2026-10-06 |

Not re-checked in this pass: cargo-fuzz nightly requirement and platform list, nextest doctest limitation, Tauri 2.12 MSRV 1.90 (from 01-desktop-architecture), App Store/Play policy on bundled SQLite and the 1–2 MB size estimate (still unverified).
