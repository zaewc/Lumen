# Ubuntu mirrors in release builds

Date: 2026-11-19

## Observed failure

The 0.14.0 [release run](https://github.com/zaewc/Lumen/actions/runs/44471937804)
failed in Linux desktop job `135383434663`, while installing Tauri's system
libraries. The runner used `mirror+file:/etc/apt/apt-mirrors.txt`, an AWS Ubuntu
mirror, and the security repository. APT timed out on those destinations and its
canonical HTTPS fallbacks; installation exited with status 100. The job allowed
only `azure.archive.ubuntu.com:80` for Ubuntu packages.

The previous successful [0.13.1 run](https://github.com/zaewc/Lumen/actions/runs/43991358270)
used the same allow-list, but downloaded all Ubuntu pockets from the Azure mirror.
This is a mismatch with the runner configuration, rather than a Rust build error.

## Primary source

GitHub's [runner image APT configuration at f2f9183](https://github.com/actions/runner-images/blob/f2f91839f985dcbde42542557c72de8d0c2970a6/images/ubuntu/scripts/build/configure-apt-sources.sh)
selects `azure.archive.ubuntu.com` or `us-east-2.ec2.archive.ubuntu.com` over HTTP as
the cloud-local primary. It adds `archive.ubuntu.com` and `security.ubuntu.com`
over HTTPS as fallback mirrors. The failed runner also attempted
`security.ubuntu.com` over HTTP from its Ubuntu sources.

## Decision

Allow these exact additional destinations in the Linux desktop release job:

- `us-east-2.ec2.archive.ubuntu.com:80`
- `archive.ubuntu.com:443`
- `security.ubuntu.com:443`
- `security.ubuntu.com:80`

Retain blocking egress, the existing Azure mirror, and APT's repository signature
verification. Do not use a wildcard, switch to audit, or alter the runner's package
sources. Rewriting the sources to Azure alone would avoid additional hosts, but
would discard GitHub's intentional mirror failover and depend on source-file layout.

## Verification

Run actionlint and zizmor, then exercise the Linux desktop release dry run on the
pull request. Only a successful production release run verifies the complete
signing, notarization, attestation, and publication path.
