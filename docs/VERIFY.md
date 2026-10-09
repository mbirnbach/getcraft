# Verifying GetCraft downloads

You don't have to take our word that a GetCraft download is genuine. This guide shows how to
check it yourself. Pick what fits your system; each check is independent.

| Check | Proves | Platforms |
|---|---|---|
| [SHA-256 checksum](#checksums-all-platforms) | The file wasn't damaged or changed after `SHA256SUMS.txt` was made | All |
| [Update signature (minisign)](#update-signatures-minisign) | The file was signed with GetCraft's release key | All |
| [Build provenance attestation](#build-provenance-github-attestations) | The file was built by GetCraft's release workflow, from a specific commit | All (releases after 0.1.3) |
| [Apple code signature](#macos-code-signature-and-notarization) | The app is signed by GetCraft's Apple Developer team and notarized by Apple | macOS |

None of these prove the code is free of bugs. They prove the file you have is the one the
project built and published.

All examples use version `0.1.3`; replace it with the version you downloaded. Download the files
from the [releases page](https://github.com/mbirnbach/getcraft/releases) only.

## Which commit and build produced a release

- **Commit:** on the release page, the tag (e.g. `v0.1.3`) and the commit next to it link to the
  exact source. In a clone: `git rev-list -n 1 v0.1.3`.
- **Build:** releases are built only by the [release workflow](../.github/workflows/release.yml),
  triggered by pushing the tag. All runs are public under
  [Actions → Release](https://github.com/mbirnbach/getcraft/actions/workflows/release.yml); a run's
  log shows every build, signing and verification step, and a release is only published if all of
  them succeeded. Releases after 0.1.3 name the commit and workflow run, and whether the macOS and
  Windows builds were signed, at the top of their release notes.
- **Files:** a release's files are the ones attached to it on the release page and listed in its
  `SHA256SUMS.txt`. They're uploaded by `github-actions[bot]`, not by hand.

Builds are not reproducible bit for bit, so building from source yourself won't give you files
with the same checksums. The attestations below are the way to link a file to its source.

## Checksums (all platforms)

Every release has a `SHA256SUMS.txt` listing the SHA-256 hash of each download. GitHub also
shows each file's SHA-256 digest next to it on the release page.

**macOS / Linux** (in the folder with your download and `SHA256SUMS.txt`):

```bash
shasum -a 256 -c --ignore-missing SHA256SUMS.txt   # macOS
sha256sum -c --ignore-missing SHA256SUMS.txt       # Linux
```

Each file you have should be reported as `OK`.

**Windows** (PowerShell, in the folder with your download and `SHA256SUMS.txt`):

```powershell
$file = "getcraft-0.1.3-windows-setup.exe"
$expected = ((Get-Content SHA256SUMS.txt) | Where-Object { $_ -like "*  $file" }).Split(" ")[0]
$actual = (Get-FileHash $file -Algorithm SHA256).Hash
if ($expected -and $actual -eq $expected) { "OK: $file matches" } else { "MISMATCH: do not run $file" }
```

> [!IMPORTANT]
> A matching checksum only shows that your file equals the one listed in `SHA256SUMS.txt`. Both
> come from the same release page, so anyone able to replace the download could replace the
> checksum too. To check *who* made the file, also verify the
> [signature](#update-signatures-minisign) or the [attestation](#build-provenance-github-attestations).

## Update signatures (minisign)

GetCraft updates itself only to files signed with its release key, using
[minisign](https://jedisct1.github.io/minisign/). Every release file has a matching `.minisig`
signature next to it (for releases after 0.1.3, `SHA256SUMS.txt` too). You can check the same
signatures yourself.

**The public key** (key ID `B92D9E0E21D756AC`):

```
RWSsVtchDp4tue2LRW4Z/MaW7HIRVXZHU/VR7geZ7RUaSkdfTivAUQ8N
```

It's stored in [`keys/update-signing.pub`](../keys/update-signing.pub) and compiled into GetCraft
from there ([`selfupdate.rs`](../crates/getcraft-core/src/selfupdate.rs)). To make sure you have
the right key, compare the copy above with that file and with the
[file's history](https://github.com/mbirnbach/getcraft/commits/main/keys/update-signing.pub):
the key was added once (replacing an empty placeholder) and hasn't changed since, and any
change would show up there. (If it ever has
to be replaced, that will be announced in the release notes.)

**Check a file** (install minisign with `brew install minisign`, `apt install minisign`, or
`scoop install minisign` on Windows):

```bash
minisign -V -P RWSsVtchDp4tue2LRW4Z/MaW7HIRVXZHU/VR7geZ7RUaSkdfTivAUQ8N \
  -m getcraft-0.1.3-macos-universal.dmg
```

This reads `getcraft-0.1.3-macos-universal.dmg.minisig` from the same folder. A good result
looks like:

```
Signature and comment signature verified
Trusted comment: timestamp:… file:getcraft-0.1.3-macos-universal.dmg …
```

The trusted comment is signed too and names the file, so a signature for one file (or an older
version) can't be passed off for another. GetCraft's self-updater checks exactly this, plus the
SHA-256 checksum and file size, and on macOS also the Apple signature described below.

The release key is held as a secret in the GitHub repository and used only by the release
workflow, which [signs the files](../.github/workflows/release.yml) and checks each signature
before publishing.

## Build provenance (GitHub attestations)

From the release after 0.1.3 on, the release workflow creates a
[GitHub artifact attestation](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations)
for each download and for `SHA256SUMS.txt`: a statement signed via GitHub (Sigstore) that the
file was built by a given workflow run, from a given commit of this repository.

With the [GitHub CLI](https://cli.github.com/):

```bash
gh attestation verify getcraft-0.2.0-macos-universal.dmg --repo mbirnbach/getcraft
```

The output names the workflow (`.github/workflows/release.yml`), the tag and the commit the file
was built from. A file that wasn't built by this repository's workflows fails the check. Add
`--signer-workflow mbirnbach/getcraft/.github/workflows/release.yml` to insist on the release
workflow specifically.

## macOS: code signature and notarization

Every normal (non-pre-) release of GetCraft for macOS is signed with the maintainer's Apple
Developer ID and notarized by Apple. GetCraft's own Apple **Team ID is `J829HHBMPW`** and its
bundle identifier is `net.brnbch.getcraft` (both in
[`trust.rs`](../crates/getcraft-core/src/trust.rs), where the self-updater requires them for
every update).

After installing to Applications:

```bash
# The signature is intact and every file is covered by it
codesign --verify --deep --strict --verbose=2 /Applications/GetCraft.app

# Gatekeeper accepts it (expect "accepted", "source=Notarized Developer ID")
spctl --assess --type execute --verbose=4 /Applications/GetCraft.app
```

**Who signed it:**

```bash
codesign -dv --verbose=4 /Applications/GetCraft.app 2>&1 | grep -E "Identifier|Authority|TeamIdentifier"
```

Check that `Identifier=net.brnbch.getcraft`, `TeamIdentifier=J829HHBMPW`, and that the
`Authority` lines read `Developer ID Application: … (J829HHBMPW)`, then
`Developer ID Certification Authority` and `Apple Root CA`.

To run the same check GetCraft runs before accepting one of its own updates:

```bash
codesign --verify --deep --verbose=1 \
  -R='identifier "net.brnbch.getcraft" and anchor apple generic and certificate leaf[subject.OU] = "J829HHBMPW"' \
  /Applications/GetCraft.app && echo "signed by GetCraft's team"
```

The DMG itself is signed and notarized too: `spctl --assess --type open --context context:primary-signature --verbose=2 getcraft-0.1.3-macos-universal.dmg`.

(The Crafting Apps GetCraft installs are checked the same way, against the ArtCraft team's Team
ID `DJ6XS33FX8`.)

## Windows: currently not code-signed

GetCraft's Windows builds (the setup and the portable zips) are **not** Authenticode-signed
today. A free certificate through [SignPath Foundation](https://signpath.org/) was applied for
and declined for now (October 2026); the signing steps are already in the release workflow and
switch on if that changes. See [SIGNING.md](SIGNING.md).

What that means for you:

- Windows SmartScreen may show *"Windows protected your PC"* when you run the setup, and the
  file's properties show no digital signature. That warning means Windows can't tell who made
  the file; it isn't a finding about the file. Choose **More info → Run anyway** to continue.
- If you'd like to be sure your download is genuine first, check the
  [checksum](#checksums-all-platforms) (catches damaged files), and the
  [minisign signature](#update-signatures-minisign) or
  [attestation](#build-provenance-github-attestations) (show it was built and signed by
  GetCraft's release process).
- Once installed, GetCraft's own updates don't rely on Windows code signing: each one must match
  its checksum and carry a valid minisign signature, or it isn't installed.

## Linux

The AppImages aren't signed with a separate AppImage signature. Verify them with the
[checksum](#checksums-all-platforms) and the [minisign signature](#update-signatures-minisign)
or [attestation](#build-provenance-github-attestations):

```bash
sha256sum -c --ignore-missing SHA256SUMS.txt
minisign -V -P RWSsVtchDp4tue2LRW4Z/MaW7HIRVXZHU/VR7geZ7RUaSkdfTivAUQ8N -m getcraft-0.1.3-linux-x86_64.AppImage
```

## Something doesn't match?

Don't run the file. Download it again from the
[releases page](https://github.com/mbirnbach/getcraft/releases); if it still fails, please
report it privately as described in [SECURITY.md](../SECURITY.md).
