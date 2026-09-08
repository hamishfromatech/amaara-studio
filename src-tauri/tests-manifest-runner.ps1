# Cargo test runner for Windows (see .cargo/config.toml).
#
# WHY: tauri-linked test binaries import comctl32 TaskDialogIndirect (v6-only,
# pulled in via the wry/menu code the lib's `run()` monomorphizes). embed-resource
# links the manifest resource into BINS only, so test executables have no
# manifest and die at load with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139).
#
# FIX: Windows probes an external `<exe>.manifest` when the image has no
# embedded manifest — so this runner copies the v6 manifest next to the test
# binary once, then hands off to the real executable.

$exe = $args[0]
if (-not $exe) { exit 1 }
$rest = @()
if ($args.Count -gt 1) { $rest = $args[1..($args.Count - 1)] }

$sidecar = Join-Path (Split-Path $exe -Parent) ((Split-Path $exe -Leaf) + ".manifest")
$template = Join-Path $PSScriptRoot "tests.manifest"
if ((Test-Path $template) -and -not (Test-Path $sidecar)) {
    Copy-Item $template $sidecar -ErrorAction SilentlyContinue
}

& $exe @rest
exit $LASTEXITCODE