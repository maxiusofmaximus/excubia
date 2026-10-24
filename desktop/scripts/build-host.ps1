# Builds the Rust core daemon (`hn-core-host`) from the core worktree and
# copies it into this app as a Tauri sidecar. Only the binary travels: the
# core sources are never copied into `desktop`.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-host.ps1
#
# Options:
#   -CoreDir   path to the core worktree (default: ..\core next to this repo)
#   -Profile   cargo profile (default: release)

param(
  [string]$CoreDir = "",
  [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"
$desktop = Split-Path -Parent $PSScriptRoot
if (-not $CoreDir) {
  $CoreDir = Join-Path (Split-Path -Parent $desktop) "core"
}
if (-not (Test-Path (Join-Path $CoreDir "Cargo.toml"))) {
  throw "no encuentro el worktree del núcleo en $CoreDir"
}

$targetDir = Join-Path $desktop "target\host"
$triple = (& rustc -vV | Select-String '^host: ').ToString().Split(' ')[1].Trim()
if (-not $triple) { throw "no pude leer el target triple de rustc" }

$env:CARGO_BUILD_JOBS = "4"
Write-Output "compilando hn-core-host + hn-mcp ($Profile) desde $CoreDir"
$buildArgs = @()
if ($Profile -eq "release") { $buildArgs += "--release" }
& cargo build @buildArgs -p hn-core-host --manifest-path (Join-Path $CoreDir "Cargo.toml") --target-dir $targetDir
if ($LASTEXITCODE -ne 0) { throw "cargo build falló con $LASTEXITCODE" }

$built = Join-Path $targetDir "$Profile\host.exe"
if (-not (Test-Path $built)) { throw "no se generó $built" }
# Alfred drives the host through this shim; without it every chat send is
# `409 seat_unavailable`.
$mcp = Join-Path $targetDir "$Profile\hn-mcp.exe"
if (-not (Test-Path $mcp)) { throw "no se generó $mcp" }

$destDir = Join-Path $desktop "src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $destDir | Out-Null
$sidecar = Join-Path $destDir "hn-core-host-$triple.exe"
Copy-Item $built $sidecar -Force
Copy-Item $mcp (Join-Path $destDir "hn-mcp-$triple.exe") -Force
# The host resolves its MCP shim by the plain name next to its own executable.
Copy-Item $mcp (Join-Path $destDir "hn-mcp.exe") -Force

# `cargo build`/`cargo run` do not lay out externalBin: leave a copy next to
# the app binaries so development runs find it too.
foreach ($appProfile in @("debug", "release")) {
  $appDir = Join-Path $desktop "target\$appProfile"
  if (Test-Path $appDir) {
    Copy-Item $built (Join-Path $appDir "hn-core-host-$triple.exe") -Force
    Copy-Item $mcp (Join-Path $appDir "hn-mcp-$triple.exe") -Force
    Copy-Item $mcp (Join-Path $appDir "hn-mcp.exe") -Force
  }
}

Write-Output "sidecar: $sidecar"
Write-Output "triple : $triple"
