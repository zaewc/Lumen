# Seroval security update

2026-10-29. The 0.6.0 version PR's OSV scan found two advisories added to
GitHub's reviewed database on October 28:

- [GHSA-9wx9-5pv9-3349](https://github.com/advisories/GHSA-9wx9-5pv9-3349):
  serialized regular expressions can end an enclosing HTML script element;
  fixed in 1.6.10.
- [GHSA-f86p-h4j2-7qgj](https://github.com/advisories/GHSA-f86p-h4j2-7qgj):
  a typed array's own constructor property can inject code into serialized
  output; fixed in 1.6.11.

The lockfile had seroval 1.6.9 through `@tanstack/router-core`. Update this
existing transitive dependency through the normal resolver, rather than add a
direct dependency or suppress either finding. The resolver selects 1.7.1 within
the parent's accepted range and the existing three-day release hold. No other
package version changes; seroval-plugins keeps 1.6.9 with its peer resolved to
the updated seroval.

[Upstream 1.7.1 notes](https://github.com/lxsmnsyc/seroval/releases/tag/seroval%401.7.1)
describe changes to rejected promises and synchronous serialization of streams.
Lumen does not import seroval directly. Its router integration, type checks,
tests and production web build must pass before release. The existing Node 24
requirement meets seroval's new Node 20 minimum. No scanner, policy, executor,
permission or AI boundary changes.
