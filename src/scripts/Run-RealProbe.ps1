param(
    [ValidateSet("models", "chat", "stream")]
    [string]$Mode = "models"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "Use-RustToolchain.ps1")
$workspace = Split-Path $PSScriptRoot -Parent
$cargoPrefix = $script:CargoPrefix
Push-Location $workspace
try {
    $env:CARGO_TARGET_DIR = $script:CargoTargetDirectory
    & $script:CargoExe @cargoPrefix build -p apiarray-runtime --bin apiarray-probe
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $executable = Join-Path $env:CARGO_TARGET_DIR "debug\apiarray-probe.exe"
    & $executable $Mode
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}

