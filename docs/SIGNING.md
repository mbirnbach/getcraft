# Signing releases

Releases are built by [`release.yml`](../.github/workflows/release.yml) when a `v*` tag is
pushed. Signing switches on by itself once the secrets below exist. A release becomes a normal release
as soon as macOS is signed and notarized; without that it's a pre-release, which GetCraft's
self-updater ignores. Windows builds are released unsigned until SignPath is set up.

Run the workflow by hand (*Actions → Release → Run workflow*) for a dry run: it builds, signs and
packages everything but publishes nothing.

## macOS: Developer ID and notarization

Needs an Apple Developer Program membership. The certificate has to be created by the account
holder.

1. **Create a Developer ID Application certificate.** In Xcode: *Settings → Accounts →* your team
   *→ Manage Certificates… → + → Developer ID Application*. (Or on
   [developer.apple.com](https://developer.apple.com/account/resources/certificates/add), which
   needs a certificate signing request from Keychain Access.)
2. **Export it as .p12.** In Keychain Access, under *My Certificates*, right-click
   *Developer ID Application: …* → *Export…* → `.p12`, with a strong password. Keep the file
   somewhere safe; it's also your backup.
3. **Create an App Store Connect API key for notarization.** In
   [App Store Connect](https://appstoreconnect.apple.com/access/integrations/api): *Users and
   Access → Integrations → Team Keys → +*, role **Developer**. Download the `AuthKey_<KEYID>.p8`
   (only possible once) and note the **Issuer ID** shown above the list.
4. **Store everything as repository secrets**, with the helper that keeps them out of your shell
   history:

   ```bash
   scripts/set-apple-secrets.sh ~/path/to/DeveloperID.p12 ~/path/to/AuthKey_XXXXXXXXXX.p8
   ```

   This sets `APPLE_CERTIFICATE_P12`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_API_KEY`,
   `APPLE_API_KEY_ID` and `APPLE_API_ISSUER_ID`.

The workflow signs the app with the hardened runtime, notarizes and staples the app and the DMG,
and checks the result with `spctl`.

## Windows: SignPath Foundation

[SignPath Foundation](https://signpath.org/) signs open-source projects for free. The
certificate is issued to "SignPath Foundation", and every release is approved by hand.

1. **Apply** at <https://signpath.org/apply> with the repository URL. The repository already
   meets their conditions: OSI license, public GitHub Actions builds, a
   [code signing policy](../README.md#code-signing-policy), a [privacy policy](../PRIVACY.md),
   uninstall instructions, and login items only after asking. Turn on two-factor authentication
   for GitHub and SignPath; they require it.
2. **After approval**, in the SignPath web app:
   - Add the predefined **GitHub.com** trusted build system and link it to the project.
   - Project slug: `getcraft`.
   - Artifact configuration slug: `windows-portable`, with the contents of
     [`.signpath/artifact-configuration.xml`](../.signpath/artifact-configuration.xml).
   - Signing policy slug: `release-signing`, with yourself as approver.
   - A CI user with *submitter* permission on that policy, and an API token for it.
3. **Store the SignPath details in GitHub:**

   ```bash
   gh secret set SIGNPATH_API_TOKEN --repo mbirnbach/getcraft
   gh variable set SIGNPATH_ORGANIZATION_ID --repo mbirnbach/getcraft --body "<organization id>"
   ```

During a release the Windows job waits (up to four hours) until you approve the signing request
in SignPath; you get an email when it's waiting.

## Making a release

1. Bump `version` in the root [`Cargo.toml`](../Cargo.toml) (the whole workspace shares it) and
   commit.
2. Tag and push: `git tag -a v0.2.0 -m "GetCraft 0.2.0" && git push origin v0.2.0`. The workflow
   refuses tags that don't match `Cargo.toml`.
3. Once SignPath is set up: approve the Windows signing request when the email arrives.
4. The release appears on GitHub. Once it's a normal (not pre-) release, the index picks it up
   within 30 minutes and installed copies of GetCraft update themselves.
