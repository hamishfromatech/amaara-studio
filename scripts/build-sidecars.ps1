# build-sidecars.ps1 — Phase 14: stage per-target-triple sidecar binaries into
# src-tauri/binaries/ so `cargo tauri build` can bundle them (externalBin).
#
# Stages:
#   1. render-worker.mjs     — already in src-tauri/binaries (verified here).
#   2. sd-server-<triple>.*  — resolved from, in order:
#        a. $env:AMAARA_SD_SERVER_BIN   (explicit path to a built sd-server)
#        b. a local stable-diffusion.cpp build tree (third_party/)
#        c. $env:AMAARA_SD_RELEASE_URL  (direct download)
#      The staged binary gets a `<file>.sha256` sidecar (hex digest) which
#      sidecar::bootstrap verifies at runtime before use.
#
# Usage:  pwsh scripts/build-sidecars.ps1            # host triple
#         pwsh scripts/build-sidecars.ps1 -Triple aarch64-apple-darwin
#
# NOTE: `cargo tauri build` expects externalBin files named
#   binaries/sd-server-<triple>(.exe). If no sd-server binary can be staged,
#   the script fails loudly — release builds require it (or remove the
#   externalBin entry and rely on download-on-first-run; see NOTES.md).
param(
  [string]$Triple = ""
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$binDir = Join-Path $root "src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

if (-not $Triple) {
  # Host triple from rustc.
  $v = rustc -vV | Select-String "^host:"
  $Triple = ($v.Line -split ": ")[1]
}
Write-Host "[build-sidecars] target triple: $Triple"

# --- 1. render worker ------------------------------------------------------
$worker = Join-Path $binDir "render-worker.mjs"
if (-not (Test-Path $worker)) { throw "render-worker.mjs missing at $worker" }
Write-Host "[build-sidecars] render-worker.mjs ok"

# --- 2. sd-server ----------------------------------------------------------
$sdName = "sd-server-$Triple.exe"
$sdDest = Join-Path $binDir $sdName

function Write-Sha256([string]$Path) {
  $h = Get-FileHash -Algorithm SHA256 $Path
  $digest = $h.Hash.ToLower()
  Set-Content -Path "$Path.sha256" -Value "$digest  $(Split-Path -Leaf $Path)"
  Write-Host "[build-sidecars] sha256 $(Split-Path -Leaf $Path): $($digest.Substring(0,16))…"
}

# a) explicit env override
if ($env:AMAARA_SD_SERVER_BIN -and (Test-Path $env:AMAARA_SD_SERVER_BIN)) {
  Copy-Item $env:AMAARA_SD_SERVER_BIN $sdDest -Force
  Write-Host "[build-sidecars] copied sd-server from $($env:AMAARA_SD_SERVER_BIN)"
}
else {
  # b) local stable-diffusion.cpp build tree
  $candidates = @(
    (Join-Path $root "third_party\stable-diffusion.cpp\build\bin\Release\sd-server.exe"),
    (Join-Path $root "third_party\stable-diffusion.cpp\build\bin\sd-server.exe"),
    (Join-Path $root "third_party\stable-diffusion.cpp\build\sd-server.exe")
  ) | Where-Object { Test-Path $_ }
  if (@($candidates).Count -gt 0) {
    $src = @($candidates)[0]
    Copy-Item $src $sdDest -Force
    Write-Host "[build-sidecars] copied sd-server from $src"
  }
  elseif ($env:AMAARA_SD_RELEASE_URL) {
    # c) direct download (e.g. a known-good release asset URL)
    Write-Host "[build-sidecars] downloading $env:AMAARA_SD_RELEASE_URL"
    Invoke-WebRequest -Uri $env:AMAARA_SD_RELEASE_URL -OutFile $sdDest
  }
  else {
    # d) build from source if the repo is checked out
    $src = Join-Path $root "third_party\stable-diffusion.cpp"
    if (Test-Path $src) {
      Write-Host "[build-sidecars] building stable-diffusion.cpp (SD_BUILD_SERVER=ON)…"
      Push-Location $src
      try {
        cmake -B build -DSD_BUILD_SERVER=ON -DSD_SERVER_BUILD_FRONTEND=OFF
        if ($LASTEXITCODE -ne 0) { throw "cmake configure failed" }
        cmake --build build --config Release --target sd-server
        if ($LASTEXITCODE -ne 0) { throw "cmake build failed" }
      } finally { Pop-Location }
      $built = Join-Path $src "build\bin\Release\sd-server.exe"
      if (-not (Test-Path $built)) { $built = Join-Path $src "build\bin\sd-server.exe" }
      if (-not (Test-Path $built)) { throw "built but sd-server.exe not found under $src\build" }
      Copy-Item $built $sdDest -Force
    }
    else {
      throw @"
No sd-server binary could be staged for $Triple.
Options:
  1. Set AMAARA_SD_SERVER_BIN to a built sd-server.exe
  2. Check out stable-diffusion.cpp under third_party/ (script builds it)
  3. Set AMAARA_SD_RELEASE_URL to a release asset URL
"@
    }
  }
}

Write-Sha256 $sdDest
Write-Host "[build-sidecars] staged: $sdDest"