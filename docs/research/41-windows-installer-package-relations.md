# Windows Installer package relations

Official documentation checked on 2026-11-21. Findings for
[ADR-0064](../decisions/0064-read-package-relations-from-each-managers-own-records.md):
why Windows has no package-relation source yet.

## Windows Installer records no dependencies between products

- Windows Installer records products and components. A component is tracked by its
  component code; "only a single instance of any component is installed", and
  several products may share it under the installer's reference counting
  ([Windows Installer Components](https://learn.microsoft.com/en-us/windows/win32/msi/windows-installer-components)).
  Removing one product leaves a component its other clients still use, so a shared
  component is not a dependent that the removal breaks.
- `MsiEnumClientsEx` lists a component's client products read-only, but needs
  administrator rights for other users' installs
  ([MsiEnumClientsEx](https://learn.microsoft.com/en-us/windows/win32/api/msi/nf-msi-msienumclientsexw)).
- There is therefore no `DependsOn` record to read. An MSI product is already listed
  as an application by its product code from its `Uninstall` entry, and its removal
  mechanism is Windows Installer (ADR-0060).

## Burn bundles record dependents, but that is WiX's convention

- Dependents between Burn bundles and packages are recorded by WiX's dependency
  extension, not by Windows Installer: a provider key under
  `Software\Classes\Installer\Dependencies\<provider>`, with a `Dependents` subkey
  naming what depends on it (WiX [`Provides`](https://wixtoolset.org/docs/schema/wxs/provides)
  and [`Requires`](https://wixtoolset.org/docs/schema/wxs/requires); Microsoft's
  [package reference counting](https://devblogs.microsoft.com/setup/introducing-package-reference-counting)).
- Only installers built with WiX write it, the provider key is not the product code,
  and its layout is documented through WiX's sources rather than a specification.
- Reading it would be a new source with its own identity mapping, an owner decision,
  and it needs a Windows host to measure. Left open.

## Elsewhere

Components shared with other products, for the removal preview, belong with #809.
