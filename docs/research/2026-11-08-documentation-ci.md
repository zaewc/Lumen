# Documentation-only CI and runner communication

## Incident

PR [#849](https://github.com/zaewc/Lumen/pull/849) changes only README files and
assets under `docs/`. Its Rust and CodeQL `changes` jobs printed `code=false`
and completed every recorded step successfully in about 14 seconds. GitHub
nevertheless marked those jobs failed 48–49 minutes later with the annotation
"The hosted runner lost communication with the server."

The classifier itself did not fail. Losing its job result left downstream jobs
without a reliable output, so their deliberate `!= 'false'` fallback ran the full
suites. The JavaScript classifier succeeded and correctly skipped its suite.

## Findings and change

The classifiers used blocking network policies allowing only `github.com:443`.
[GitHub's runner communication requirements](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#communication-requirements-for-github-hosted-runners)
also require the API, Actions service and results/log-storage endpoints. GitHub
warns that firewall rules may need to cover their CNAME destinations.

An incomplete allowlist is consistent with the connection-loss annotation and
the network drops in the agent logs. The logs do not prove which connection
ultimately prevented job completion. Keep blocking enabled and add the documented
runner endpoints, the action download endpoint and the results receiver's CNAME.
Bound classifier jobs to five minutes. A successful subsequent CI run is needed
to validate this mitigation.

For documentation-only PRs, skip Rust formatting and schema drift, and skip
checkout/hardening in the otherwise empty Rust and CodeQL matrix jobs. Preserve
the individual required matrix check names and run `cargo xtask lint` for
documentation links and ADR consistency. Unknown classifier results still run
code checks. Non-PR events still run the full suites.

Supply-chain scans, secret scanning, workflow lint and PR metadata checks remain
in place under [ADR-0026](../decisions/0026-ci-and-supply-chain-security.md).
