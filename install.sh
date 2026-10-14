#!/bin/sh

set -eu

repository="https://github.com/trygantry/gantry-distribution"
version="${GANTRY_VERSION:-latest}"
install_dir="${GANTRY_INSTALL_DIR:-${HOME}/.local/bin}"
os="${GANTRY_OS:-$(uname -s)}"
arch="${GANTRY_ARCH:-$(uname -m)}"

case "$os" in
  Darwin)
    case "$arch" in
      arm64|aarch64) asset="gantry-macos-aarch64" ;;
      x86_64|amd64) asset="gantry-macos-x86_64" ;;
      *)
        echo "Gantry does not publish a macOS CLI for architecture '$arch'." >&2
        exit 1
        ;;
    esac
    ;;
  Linux)
    case "$arch" in
      x86_64|amd64) asset="gantry-linux-x86_64" ;;
      *)
        echo "Gantry currently publishes the Linux CLI for x86_64 only; detected '$arch'." >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "This installer supports macOS and Linux. On Windows, run install.ps1 in PowerShell." >&2
    exit 1
    ;;
esac

if [ -n "${GANTRY_DOWNLOAD_BASE:-}" ]; then
  download_base="$GANTRY_DOWNLOAD_BASE"
elif [ "$version" = "latest" ]; then
  download_base="$repository/releases/latest/download"
else
  case "$version" in
    v*) tag="$version" ;;
    *) tag="v$version" ;;
  esac
  download_base="$repository/releases/download/$tag"
fi

download() {
  source_url="$1"
  destination="$2"
  if command -v curl >/dev/null 2>&1; then
    curl --fail --location --silent --show-error "$source_url" --output "$destination"
  elif command -v wget >/dev/null 2>&1; then
    wget --quiet "$source_url" --output-document="$destination"
  else
    echo "Install curl or wget, then run the Gantry installer again." >&2
    exit 1
  fi
}

temp_root="${TMPDIR:-/tmp}"
temp_dir="$(mktemp -d "$temp_root/gantry-install.XXXXXX")"
trap 'rm -rf "$temp_dir"' EXIT HUP INT TERM

binary_path="$temp_dir/$asset"
checksum_path="$temp_dir/$asset.sha256"
download "$download_base/$asset" "$binary_path"
download "$download_base/$asset.sha256" "$checksum_path"

expected="$(awk 'NR == 1 { print $1 }' "$checksum_path")"
case "$expected" in
  *[!0-9a-fA-F]*|'')
    echo "The published checksum for $asset is malformed." >&2
    exit 1
    ;;
esac
if [ "${#expected}" -ne 64 ]; then
  echo "The published checksum for $asset is malformed." >&2
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$binary_path" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$binary_path" | awk '{ print $1 }')"
else
  echo "A SHA-256 utility (sha256sum or shasum) is required to verify Gantry." >&2
  exit 1
fi

if [ "$actual" != "$expected" ]; then
  echo "Checksum verification failed for $asset; Gantry was not installed." >&2
  exit 1
fi

mkdir -p "$install_dir"
target="$install_dir/gantry"
if command -v install >/dev/null 2>&1; then
  install -m 0755 "$binary_path" "$target"
else
  cp "$binary_path" "$target"
  chmod 0755 "$target"
fi

echo "Installed Gantry at $target"
"$target" --version

case ":${PATH:-}:" in
  *:"$install_dir":*) ;;
  *)
    echo "Add Gantry to your PATH:"
    echo "  export PATH=\"$install_dir:\$PATH\""
    ;;
esac
