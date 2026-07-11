param(
    [Parameter(Mandatory = $true)]
    [string]$InputFile
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "Use-RustToolchain.ps1")
$workspace = Split-Path $PSScriptRoot -Parent
$cargoPrefix = $script:CargoPrefix
$resolvedInput = (Resolve-Path $InputFile).Path
Push-Location $workspace
try {
    $env:CARGO_TARGET_DIR = $script:CargoTargetDirectory
    & $script:CargoExe @cargoPrefix build -p apiarray-runtime --bin apiarray-control
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $executable = Join-Path $env:CARGO_TARGET_DIR "debug\apiarray-control.exe"
    Get-Content -Raw -Encoding UTF8 $resolvedInput | & $executable
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
