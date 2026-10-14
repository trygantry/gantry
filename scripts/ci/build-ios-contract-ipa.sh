#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "The iOS contract IPA builder requires macOS and codesign." >&2
  exit 2
fi
if ! command -v codesign >/dev/null 2>&1; then
  echo "codesign is required to build the iOS contract IPA." >&2
  exit 2
fi

repository_root="$(cd "$(dirname "$0")/../.." && pwd)"
fixture_root="${repository_root}/fixtures/mobile/swift-ios-minimal"
output="${1:-${fixture_root}/build/GantryFixture.ipa}"
work="$(mktemp -d -t gantry-ios-contract.XXXXXX)"
trap 'rm -rf "$work"' EXIT

app="${work}/Payload/GantryFixture.app"
mkdir -p "$app" "$(dirname "$output")"
cp "${fixture_root}/ios/GantryFixture/Info.plist" "$app/Info.plist"
cp "${fixture_root}/ios/GantryFixture/PrivacyInfo.xcprivacy" "$app/PrivacyInfo.xcprivacy"
cp /usr/bin/true "$app/GantryFixture"
chmod 755 "$app/GantryFixture"

codesign --force --sign - \
  --entitlements "${fixture_root}/ios/GantryFixture/GantryFixture.entitlements" \
  "$app"
codesign --verify --deep --strict --verbose=2 "$app"

rm -f "$output"
(
  cd "$work"
  /usr/bin/zip -X -qry "$output" Payload
)
printf '%s\n' "$output"
