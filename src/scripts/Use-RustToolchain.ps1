Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (Get-Command link.exe -ErrorAction SilentlyContinue) {
    $script:CargoExe = "cargo"
    $script:CargoPrefix = @()
    $script:CargoTargetDirectory = "target\msvc"
    return
}

$mingwBin = Join-Path $env:LOCALAPPDATA "Programs\CLion\bin\mingw\bin"
if (-not (Test-Path (Join-Path $mingwBin "gcc.exe"))) {
    throw "Neither MSVC link.exe nor CLion MinGW was found. Install Visual Studio C++ Build Tools or provide MinGW."
}

$installed = rustup toolchain list
if (-not ($installed -match "stable-x86_64-pc-windows-gnu")) {
    throw "Missing stable-x86_64-pc-windows-gnu. Run: rustup toolchain install stable-x86_64-pc-windows-gnu --profile minimal"
}

$env:PATH = "$mingwBin;$env:PATH"
$script:CargoExe = "cargo"
$script:CargoPrefix = @("+stable-x86_64-pc-windows-gnu")
$script:CargoTargetDirectory = "target\gnu"
