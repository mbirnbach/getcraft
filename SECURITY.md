# Security

GetCraft downloads and installs programs, so security problems matter a lot here. Thank you for
reporting them.

## Reporting a vulnerability

Please **don't open a public issue**. Report it privately through GitHub:
[Security → Report a vulnerability](https://github.com/mbirnbach/getcraft/security/advisories/new).

Include what you found, how to reproduce it, and which version and system you used. You'll get
an answer within a few days; fixes ship as a new release, and installed copies update
themselves.

Problems in the Crafting Apps themselves (PhotoCraft and the others) belong to their own
repositories under [github.com/storytold](https://github.com/storytold).

## Supported versions

Only the latest release gets fixes. GetCraft updates itself, so that's what almost everyone runs.

## How GetCraft protects downloads

See [How downloads are verified](README.md#how-downloads-are-verified) in the README. To check a
GetCraft download yourself (checksums, update signatures, build provenance, Apple signature), see
[docs/VERIFY.md](docs/VERIFY.md).

Dependencies are kept up to date by Dependabot and checked against the RustSec advisory database;
the code and workflows are scanned with CodeQL ([security checks](.github/workflows/security.yml)).
These catch known kinds of problems, not every possible one.
