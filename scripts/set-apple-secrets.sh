#!/usr/bin/env bash
# Stores the macOS signing material as GitHub Actions secrets of this repository, for the
# release workflow. Run it yourself; nothing is printed or written to disk.
#
#   scripts/set-apple-secrets.sh path/to/DeveloperID.p12 path/to/AuthKey_XXXXXXXXXX.p8
#
# You'll be asked for the .p12 password, the API key ID and the issuer ID. Requires `gh`
# logged in with access to the repository.
set -euo pipefail

P12="${1:?usage: $0 <certificate.p12> <AuthKey.p8>}"
P8="${2:?usage: $0 <certificate.p12> <AuthKey.p8>}"
REPO="${GETCRAFT_REPO:-mbirnbach/getcraft}"

[[ -f "$P12" ]] || { echo "not found: $P12" >&2; exit 1; }
[[ -f "$P8" ]] || { echo "not found: $P8" >&2; exit 1; }

read -r -s -p "Password of $(basename "$P12"): " P12_PASSWORD; echo
export P12_PASSWORD # read by openssl via env:, so it never shows up in the process list
# Check the password and that the file really holds a Developer ID Application identity.
if ! openssl pkcs12 -in "$P12" -passin env:P12_PASSWORD -nokeys -legacy 2>/dev/null | grep -q "Developer ID Application" \
  && ! openssl pkcs12 -in "$P12" -passin env:P12_PASSWORD -nokeys 2>/dev/null | grep -q "Developer ID Application"; then
  echo "That .p12 doesn't contain a 'Developer ID Application' certificate, or the password is wrong." >&2
  exit 1
fi

# The key ID is also part of the file name Apple gives the key: AuthKey_<KEYID>.p8
DEFAULT_KEY_ID=$(basename "$P8" | sed -nE 's/^AuthKey_([A-Z0-9]+)\.p8$/\1/p')
read -r -p "App Store Connect API key ID [${DEFAULT_KEY_ID}]: " KEY_ID
KEY_ID="${KEY_ID:-$DEFAULT_KEY_ID}"
read -r -p "App Store Connect issuer ID: " ISSUER_ID

base64 -i "$P12" | gh secret set APPLE_CERTIFICATE_P12 --repo "$REPO"
printf '%s' "$P12_PASSWORD" | gh secret set APPLE_CERTIFICATE_PASSWORD --repo "$REPO"
base64 -i "$P8" | gh secret set APPLE_API_KEY --repo "$REPO"
printf '%s' "$KEY_ID" | gh secret set APPLE_API_KEY_ID --repo "$REPO"
printf '%s' "$ISSUER_ID" | gh secret set APPLE_API_ISSUER_ID --repo "$REPO"
unset P12_PASSWORD

echo "Done. Secrets now set on $REPO:"
gh secret list --repo "$REPO"
