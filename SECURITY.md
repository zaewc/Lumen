# Security policy

Lumen inspects and, with explicit consent, moves files on users' devices. Security and
data-safety bugs are treated as the highest priority.

## Reporting a vulnerability

Please report vulnerabilities **privately** through GitHub's
[private vulnerability reporting](https://github.com/zaewc/Lumen/security/advisories/new)
for this repository. Do not open a public issue for security problems.

Include the affected component, version or commit, platform and OS version,
reproduction steps, and impact. We aim to acknowledge reports within 3 business days.

## Supported versions

Lumen is pre-release. Only the `main` branch is supported until the first release;
this section will list supported release lines afterwards.

## Especially in scope

- Any way to make Lumen move, delete or overwrite a file the user did not confirm, or
  a file other than the one shown (symlink/junction races, path confusion, hard-link
  tricks).
- Bypassing the safety policy, including through AI prompt injection.
- Defeating reversibility (quarantine or ledger tampering, restore overwrites).
- Unauthenticated access to local IPC or the optional local dashboard.
- Disclosure of file names, paths, contents or hashes beyond what the user enabled.
- Supply-chain or release-pipeline weaknesses.

## Design references

- [Threat model](docs/security/threat-model.md)
- [ADR-0016: single handle-relative executor](docs/decisions/0016-single-handle-relative-executor.md)
- [ADR-0023: IPC security](docs/decisions/0023-ipc-security.md)
- [ADR-0026: CI and supply-chain security](docs/decisions/0026-ci-and-supply-chain-security.md)
