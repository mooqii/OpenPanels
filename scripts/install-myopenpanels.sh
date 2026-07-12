#!/usr/bin/env sh
set -eu

REPO="${MYOPENPANELS_INSTALL_REPO:-mooqii/OpenPanels}"
DEFAULT_MANIFEST_URL="https://github.com/$REPO/releases/latest/download/myopenpanels-manifest.json"
MANIFEST_URL="${MYOPENPANELS_INSTALL_MANIFEST_URL:-${MYOPENPANELS_UPDATE_MANIFEST_URL:-$DEFAULT_MANIFEST_URL}}"
INSTALL_DIR="${MYOPENPANELS_INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="myopenpanels"

fail() {
  printf 'myopenpanels install failed: %s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

detect_target() {
  os="$(uname -s 2>/dev/null | tr '[:upper:]' '[:lower:]')"
  machine="$(uname -m 2>/dev/null | tr '[:upper:]' '[:lower:]')"
  case "$machine" in
    arm64 | aarch64) arch="aarch64" ;;
    x86_64 | amd64) arch="x86_64" ;;
    *) fail "unsupported CPU architecture: $machine" ;;
  esac
  case "$os" in
    darwin) printf '%s-apple-darwin' "$arch" ;;
    linux) fail "Linux release packages are temporarily disabled" ;;
    *) fail "unsupported operating system: $os" ;;
  esac
}

json_asset_field() {
  target="$1"
  manifest="$2"
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$target" "$manifest" <<'PY'
import json
import sys
target, path = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as handle:
    manifest = json.load(handle)
asset = manifest.get("assets", {}).get(target)
if not asset:
    sys.exit(1)
print("{}\t{}\t{}".format(asset.get("url", ""), asset.get("sha256", ""), asset.get("size", "")))
PY
    return
  fi
  awk -v target="\"$target\"" '
    $0 ~ target { in_target = 1; next }
    in_target && /}/ { exit }
    in_target && /"url"[[:space:]]*:/ {
      line = $0
      sub(/.*"url"[[:space:]]*:[[:space:]]*"/, "", line)
      sub(/".*/, "", line)
      url = line
    }
    in_target && /"sha256"[[:space:]]*:/ {
      line = $0
      sub(/.*"sha256"[[:space:]]*:[[:space:]]*"/, "", line)
      sub(/".*/, "", line)
      sha = line
    }
    in_target && /"size"[[:space:]]*:/ {
      line = $0
      sub(/.*"size"[[:space:]]*:[[:space:]]*/, "", line)
      sub(/[, ].*/, "", line)
      size = line
    }
    END {
      if (url != "" && sha != "") {
        printf "%s\t%s\t%s\n", url, sha, size
      } else {
        exit 1
      }
    }
  ' "$manifest"
}

sha256_file() {
  path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  else
    fail "missing sha256sum or shasum"
  fi
}

file_size() {
  wc -c <"$1" | tr -d '[:space:]'
}

need curl
need awk
need mktemp

TARGET="$(detect_target)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

MANIFEST_PATH="$TMP_DIR/manifest.json"
ARCHIVE_PATH="$TMP_DIR/myopenpanels-asset"
EXTRACT_DIR="$TMP_DIR/extract"

printf 'Installing myopenpanels for %s\n' "$TARGET"
curl -fsSL "$MANIFEST_URL" -o "$MANIFEST_PATH"

ASSET="$(json_asset_field "$TARGET" "$MANIFEST_PATH")" || fail "no release asset for $TARGET in $MANIFEST_URL"
ASSET_URL="$(printf '%s' "$ASSET" | awk -F '\t' '{print $1}')"
EXPECTED_SHA="$(printf '%s' "$ASSET" | awk -F '\t' '{print $2}')"
EXPECTED_SIZE="$(printf '%s' "$ASSET" | awk -F '\t' '{print $3}')"

[ -n "$ASSET_URL" ] || fail "manifest asset for $TARGET has no url"
[ -n "$EXPECTED_SHA" ] || fail "manifest asset for $TARGET has no sha256"

curl -fsSL "$ASSET_URL" -o "$ARCHIVE_PATH"

ACTUAL_SHA="$(sha256_file "$ARCHIVE_PATH")"
[ "$(printf '%s' "$ACTUAL_SHA" | tr '[:upper:]' '[:lower:]')" = "$(printf '%s' "$EXPECTED_SHA" | tr '[:upper:]' '[:lower:]')" ] ||
  fail "checksum mismatch for downloaded archive"

if [ -n "${EXPECTED_SIZE:-}" ] && [ "$EXPECTED_SIZE" != "0" ]; then
  ACTUAL_SIZE="$(file_size "$ARCHIVE_PATH")"
  [ "$ACTUAL_SIZE" = "$EXPECTED_SIZE" ] || fail "size mismatch for downloaded archive"
fi

mkdir -p "$EXTRACT_DIR"
case "$ASSET_URL" in
  *.zip)
    need unzip
    unzip -q "$ARCHIVE_PATH" -d "$EXTRACT_DIR"
    ;;
  *)
    need tar
    tar -xzf "$ARCHIVE_PATH" -C "$EXTRACT_DIR"
    ;;
esac

EXTRACTED_BINARY="$(find "$EXTRACT_DIR" -type f -name "$BINARY_NAME" | head -n 1)"
[ -n "$EXTRACTED_BINARY" ] || fail "release archive does not contain $BINARY_NAME"

mkdir -p "$INSTALL_DIR"
INSTALL_PATH="$INSTALL_DIR/$BINARY_NAME"
cp "$EXTRACTED_BINARY" "$INSTALL_PATH"
chmod 755 "$INSTALL_PATH"

printf 'Installed %s to %s\n' "$BINARY_NAME" "$INSTALL_PATH"
"$INSTALL_PATH" --version

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    printf '\n%s is not currently on PATH.\n' "$INSTALL_DIR"
    printf 'Add this to your shell profile, then restart your shell:\n'
    printf '  export PATH="%s:$PATH"\n' "$INSTALL_DIR"
    ;;
esac
