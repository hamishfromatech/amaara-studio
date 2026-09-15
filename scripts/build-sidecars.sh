#!/usr/bin/env bash
# build-sidecars.sh — Phase 14: stage per-target-triple sidecar binaries into
# src-tauri/binaries/ so `cargo tauri build` can bundle them (externalBin).
#
# Same contract as build-sidecars.ps1 (see that file for resolution order):
#   AMAARA_SD_SERVER_BIN > third_party build tree > AMAARA_SD_RELEASE_URL >
#   build stable-diffusion.cpp from source. Writes a .sha256 sidecar.
#
# Usage: ./scripts/build-sidecars.sh [target-triple]   # default: host triple
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
bin_dir="$root/src-tauri/binaries"
mkdir -p "$bin_dir"

triple="${1:-}"
if [ -z "$triple" ]; then
  triple="$(rustc -vV | sed -n 's/^host: //p')"
fi
echo "[build-sidecars] target triple: $triple"

# --- 1. render worker ------------------------------------------------------
worker="$bin_dir/render-worker.mjs"
[ -f "$worker" ] || { echo "render-worker.mjs missing at $worker" >&2; exit 1; }
echo "[build-sidecars] render-worker.mjs ok"

# --- 2. sd-server ----------------------------------------------------------
ext=""; [ "$(uname -s)" = "Darwin" ] && ext=""        # plain binary
sd_name="sd-server-$triple$ext"
sd_dest="$bin_dir/$sd_name"

write_sha256() {
  local p="$1" digest
  digest="$(shasum -a 256 "$p" | awk '{print $1}')"
  printf '%s  %s\n' "$digest" "$(basename "$p")" > "$p.sha256"
  echo "[build-sidecars] sha256 $(basename "$p"): ${digest:0:16}…"
}

if [ -n "${AMAARA_SD_SERVER_BIN:-}" ] && [ -f "$AMAARA_SD_SERVER_BIN" ]; then
  cp -f "$AMAARA_SD_SERVER_BIN" "$sd_dest"
  echo "[build-sidecars] copied sd-server from $AMAARA_SD_SERVER_BIN"
else
  # b) local stable-diffusion.cpp build tree
  candidate=""
  for c in \
    "$root/third_party/stable-diffusion.cpp/build/bin/Release/sd-server" \
    "$root/third_party/stable-diffusion.cpp/build/bin/sd-server" \
    "$root/third_party/stable-diffusion.cpp/build/sd-server"; do
    [ -f "$c" ] && { candidate="$c"; break; }
  done
  if [ -n "$candidate" ]; then
    cp -f "$candidate" "$sd_dest"
    echo "[build-sidecars] copied sd-server from $candidate"
  elif [ -n "${AMAARA_SD_RELEASE_URL:-}" ]; then
    echo "[build-sidecars] downloading $AMAARA_SD_RELEASE_URL"
    curl -fL "$AMAARA_SD_RELEASE_URL" -o "$sd_dest"
  else
    # d) build from source if the repo is checked out
    src="$root/third_party/stable-diffusion.cpp"
    if [ -d "$src" ]; then
      echo "[build-sidecars] building stable-diffusion.cpp (SD_BUILD_SERVER=ON)…"
      cmake -B "$src/build" -S "$src" -DSD_BUILD_SERVER=ON -DSD_SERVER_BUILD_FRONTEND=OFF
      cmake --build "$src/build" --config Release --target sd-server
      built="$src/build/bin/Release/sd-server"
      [ -f "$built" ] || built="$src/build/bin/sd-server"
      [ -f "$built" ] || { echo "built but sd-server not found under $src/build" >&2; exit 1; }
      cp -f "$built" "$sd_dest"
    else
      cat >&2 <<'EOS'
No sd-server binary could be staged.
Options:
  1. Set AMAARA_SD_SERVER_BIN to a built sd-server binary
  2. Check out stable-diffusion.cpp under third_party/ (script builds it)
  3. Set AMAARA_SD_RELEASE_URL to a release asset URL
EOS
      exit 1
    fi
  fi
fi

write_sha256 "$sd_dest"
chmod +x "$sd_dest" 2>/dev/null || true
echo "[build-sidecars] staged: $sd_dest"