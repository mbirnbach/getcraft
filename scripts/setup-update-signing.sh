#!/usr/bin/env bash
# Creates the minisign key that signs GetCraft releases for its self-updater. Run it once,
# yourself; the private key never leaves your machine except into the GitHub secret.
#
#   scripts/setup-update-signing.sh [key folder, default ~/.getcraft-signing]
#
# Writes the public key to keys/update-signing.pub (commit it), stores the private key as the
# MINISIGN_SECRET_KEY secret of the repository, and keeps a copy in the key folder: back that up
# (e.g. in your password manager). If it's lost, installed copies can't verify future updates and
# users have to download GetCraft again by hand.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="${GETCRAFT_REPO:-mbirnbach/getcraft}"
DIR="${1:-$HOME/.getcraft-signing}"
SECRET="$DIR/getcraft-update.key"

command -v minisign >/dev/null || { echo "minisign isn't installed: brew install minisign" >&2; exit 1; }
if [[ -e "$SECRET" ]]; then
  echo "$SECRET already exists; not replacing an existing key." >&2
  exit 1
fi
if grep -q '^RW' keys/update-signing.pub; then
  echo "keys/update-signing.pub already holds a key. Replacing it would stop installed copies" >&2
  echo "from updating; remove it by hand if you really mean to rotate the key." >&2
  exit 1
fi

mkdir -p "$DIR" && chmod 700 "$DIR"
# -W: no password, so the release workflow can sign without one; the secret is protected by
# GitHub (and your backup by its own storage).
minisign -G -W -p keys/update-signing.pub -s "$SECRET" -f
chmod 600 "$SECRET"

gh secret set MINISIGN_SECRET_KEY --repo "$REPO" < "$SECRET"

echo
echo "Done."
echo "  Public key:  keys/update-signing.pub  (commit this)"
echo "  Private key: $SECRET  (back it up somewhere safe, then you may delete it here)"
echo "  GitHub secret MINISIGN_SECRET_KEY is set on $REPO."
