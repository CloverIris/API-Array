Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "Use-RustToolchain.ps1")
$workspace = Split-Path $PSScriptRoot -Parent
$cargoPrefix = $script:CargoPrefix
Push-Location $workspace
try {
    $env:CARGO_TARGET_DIR = $script:CargoTargetDirectory

    & $script:CargoExe @cargoPrefix fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    & $script:CargoExe @cargoPrefix clippy --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    & $script:CargoExe @cargoPrefix test --workspace --all-targets
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
