#!/usr/bin/env bash
set -euo pipefail

app_path="${1:?Usage: smoke-macos-bundle.sh /path/to/Gantry.app}"
archive_path="${app_path}.tar.gz"
signature_path="${archive_path}.sig"
is_signed="${IS_SIGNED:-true}"

if [[ ! -d "$app_path" ]]; then
  echo "Expected packaged application at $app_path" >&2
  exit 1
fi

if [[ "$is_signed" == "true" ]]; then
  if [[ ! -s "$archive_path" || ! -s "$signature_path" ]]; then
    echo "Expected signed updater archive and signature beside $app_path" >&2
    exit 1
  fi

  codesign --verify --deep --strict --verbose=2 "$app_path"
  xcrun stapler validate "$app_path"
  spctl --assess --type execute --verbose=2 "$app_path"
else
  echo "Skipping strict codesign/stapler verification for unsigned desktop build."
fi

open -n "$app_path"
app_pid=""
for _ in {1..20}; do
  app_pid="$(pgrep -x gantry-desktop | head -n 1 || true)"
  [[ -n "$app_pid" ]] && break
  sleep 1
done

if [[ -z "$app_pid" ]]; then
  echo "The Gantry application did not reach a running process." >&2
  exit 1
fi

kill "$app_pid"
wait "$app_pid" 2>/dev/null || true
echo "Gantry bundle launched successfully."
