# ADR-0042: Ship a built-in Jeb key in release builds

- Status: Accepted (owner decision, 2026-10-26)
- Date: 2026-10-26
- Amends: [ADR-0036](0036-jeb-on-the-school-ai-hub.md) (whose key is used),
  [ADR-0037](0037-keep-ai-review-on-once-turned-on.md) (turning it on no longer needs a
  stored key)

## Context and problem statement

AI review sends metadata about proposed quarantines to Jeb on the school's AI Hub
(ADR-0036), with an API key the user stores in the operating system's secret store.
Jeb keys are issued per student, so anyone without a school account cannot use AI
review at all, and everyone else has to find, copy and paste a key first.

The owner decided that release builds should carry one Jeb key of their own, so people
can turn AI review on without a key. Before deciding, the owner was told, and accepted,
that a key compiled into a public binary can be read out of it by anyone, and that it
may have to be revoked.

## Decision drivers

- AI review becomes usable without a school account.
- Opt-in stays opt-in (ADR-0020, ADR-0037): having a key never turns AI review on.
- The key never appears in the repository, in pull request runs, in logs, errors,
  traces or anything the interface receives.
- A user's own key keeps working and wins, so nobody depends on the shared quota who
  does not want to.
- Builds without the key (local builds, CI checks, pull request dry runs) behave
  exactly as before.
- Honesty: say plainly what a built-in key does and does not protect.

## Considered options

1. **Compile the key into release builds from a protected CI secret**, used only when
   the user has stored no key.
2. **A Lumen-run proxy** that holds the key and forwards requests. It hides the key,
   but anyone can still call the proxy, so the quota is spent the same way; it adds a
   server to run and secure, and a second party that sees the metadata.
3. **Obfuscate the key in the binary** (split, XOR, encrypt with a key also in the
   binary). Rejected: the program must reconstruct the key to send it, so whoever can
   run the binary can recover it (from memory, a debugger or the TLS client). It would
   only make the risk look smaller than it is.
4. **Keep requiring a key** (the status quo). Rejected by the owner.

## Decision outcome

Chosen option: **1**.

- **Injection:** `lumen_app::ai::builtin_key()` reads `LUMEN_JEB_BUILTIN_KEY` with
  `option_env!` at compile time. That is the only place the variable is read. An unset
  or empty variable, or one that is not a valid API key, means "no built-in key", and
  the build behaves exactly as before. The key is never committed, and no source file,
  test or fixture contains it; tests pass a stand-in to functions that take the
  built-in key as a parameter.
- **Where it comes from:** a GitHub Actions secret `JEB_API_KEY` in the `release`
  environment, which the owner sets (`docs/operations/release.md`). Only tag-triggered
  release runs use that environment, and only the steps that compile the command line
  and the desktop app receive the secret, as an environment variable. Pull request dry
  runs never reference the environment and pass an empty value, so they never get it.
  The owner restricts the environment to `v*` tags, so no other ref can use it. A tag
  run without the secret fails rather than publishing a build without the key.
- **Precedence:** a key the user stored wins (Jeb's, then `OpenRouter`'s, as in
  ADR-0036). The built-in Jeb key is used only when none is stored. A stored key that
  cannot be read or is invalid is an error, as before; Lumen does not fall back to the
  built-in key, so a user's choice of service is never silently replaced. Where the
  platform has no secret store yet (Linux), the built-in key is the only one.
- **Consent is unchanged:** AI review starts off and is turned on only through the
  consent dialog or `lumen ai on`, with the same disclosure, now naming Lumen's
  built-in key when that is the one used. With a built-in key the dialog no longer
  asks for one, but still offers to store the user's own key.
- **Never shown:** the key is an `ApiKey`, whose `Debug` output is redacted; errors,
  logs and judge traces never contain it. Views say only where the key comes from
  (`key_source`: `builtin`, `stored` or `null`). `lumen jev key status` and
  `lumen ai status` say "built-in key available", never the key.
- **Not obfuscated:** the key sits in the binary as plain text. This ADR says so
  instead of pretending otherwise.

### Consequences

- Good: anyone with a release build can turn AI review on in one step.
- Good: local and CI builds, and every user who stores a key, are unaffected.
- Bad: **the key is extractable.** Anyone who downloads a release can read it from the
  binary (for example with `strings`) and spend its quota, for Lumen or for anything
  else the AI Hub allows. Requests made with it count against the owner's school
  account.
- Bad: **revoking it disables AI review in every build that carries it.** Those builds
  then answer "unavailable", and the policy decides alone (ADR-0020) until a new
  release ships a new key, or the user stores their own. Rotation means a new release.
- Bad: the Jeb operator (the school) has not been asked whether a student key may be
  shared this way. **The owner should confirm the operator's permission** before
  publishing a release that carries the key, and stop if it is refused.
- Bad: a shared key concentrates load. Jeb allows one request at a time per account
  and has per-account quotas (ADR-0036), and each assessment may send one request per
  proposed item plus up to 20 descriptions of review items (ADR-0041). Many users at
  once will see "unavailable" more often; the decision then proceeds without the
  judge, which is safe but less useful.
- Bad: the release jobs' build steps, including dependency build scripts and the
  desktop app's web build, run with the secret in their environment. On Linux the
  command-line job blocks egress except to the toolchain, crates.io and GitHub; the
  others do not enforce that. The key is public in the binary anyway, so this adds
  little, but it is one more reason not to reuse the key for anything else.

### Mitigations

- The user's own key wins, and the interface still offers to store one.
- The disclosure says when Lumen's built-in key is used, and that the school's
  operator may keep requests (ADR-0036).
- The key is only compiled into tag builds from a protected environment; pull request
  runs never receive it, and it is masked in workflow logs.
- Every failure of the judge (revoked key, exhausted quota, busy server) means "no
  judgment", never a less cautious decision (ADR-0020).
- The owner can revoke the key on the AI Hub at any time, set a new one with
  `gh secret set JEB_API_KEY --env release`, and cut a new release.

## More information

- [ADR-0020](0020-jev-evidence-only-judge.md), [ADR-0026](0026-ci-and-supply-chain-security.md),
  [ADR-0036](0036-jeb-on-the-school-ai-hub.md), [ADR-0037](0037-keep-ai-review-on-once-turned-on.md)
- [`docs/operations/release.md`](../operations/release.md) (setting and rotating the
  secret)
- GitHub Docs, "Using environments for deployment" and "Using secrets in GitHub
  Actions": environment secrets are available only to jobs that reference the
  environment, after its protection rules pass.
