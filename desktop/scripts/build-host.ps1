# Builds the Rust core daemon (`excubia-host`) from the core worktree and
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
$CoreDir = (Resolve-Path $CoreDir).Path
$targetDir = Join-Path $desktop "target\host"
$triple = (& rustc -vV | Select-String '^host: ').ToString().Split(' ')[1].Trim()
if (-not $triple) { throw "no pude leer el target triple de rustc" }

$env:CARGO_BUILD_JOBS = "4"
Write-Output "compilando excubia-host + excubia-mcp + excubia-media-worker ($Profile, $triple) desde $CoreDir"
$buildArgs = @()
if ($Profile -eq "release") { $buildArgs += "--release" }
# Cargo lee .cargo/config.toml desde el directorio de trabajo: compilar dentro de
# desktop/ hace que el núcleo hermano use el mismo CRT estático que la app, desde
# cualquier carpeta. Push/Pop limita el cambio de directorio a este bloque.
Push-Location $desktop
try {
  & cargo build @buildArgs --target $triple -p excubia-host -p excubia-seat -p excubia-media --bins --manifest-path (Join-Path $CoreDir "Cargo.toml") --target-dir $targetDir
  $cargoExit = $LASTEXITCODE
} finally {
  Pop-Location
}
if ($cargoExit -ne 0) { throw "cargo build falló con $cargoExit" }

$outDir = Join-Path $targetDir "$triple\$Profile"
$built = Join-Path $outDir "excubia-host.exe"
if (-not (Test-Path $built)) { throw "no se generó $built" }
# Alfred drives the host through this shim; without it every chat send is
# `409 seat_unavailable`.
$mcp = Join-Path $outDir "excubia-mcp.exe"
if (-not (Test-Path $mcp)) { throw "no se generó $mcp" }

# Every image attachment is decoded in this child process, next to the host;
# without it each upload fails with `500 internal error`.
$worker = Join-Path $outDir "excubia-media-worker.exe"
if (-not (Test-Path $worker)) { throw "no se generó $worker" }

# The host refuses to start Alfred's seat without the approval hook next to it.
$hook = Join-Path $outDir "excubia-uac-hook.exe"
if (-not (Test-Path $hook)) { throw "no se generó $hook" }

$destDir = Join-Path $desktop "src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $destDir | Out-Null
$sidecar = Join-Path $destDir "excubia-host-$triple.exe"
Copy-Item $built $sidecar -Force
Copy-Item $mcp (Join-Path $destDir "excubia-mcp-$triple.exe") -Force
# The host resolves its MCP shim by the plain name next to its own executable.
Copy-Item $mcp (Join-Path $destDir "excubia-mcp.exe") -Force
Copy-Item $hook (Join-Path $destDir "excubia-uac-hook-$triple.exe") -Force
Copy-Item $hook (Join-Path $destDir "excubia-uac-hook.exe") -Force
Copy-Item $worker (Join-Path $destDir "excubia-media-worker-$triple.exe") -Force
Copy-Item $worker (Join-Path $destDir "excubia-media-worker.exe") -Force

# The OpenCode UAC plugin (plain JS, no dependencies) travels as a bundled
# resource; the host verifies it with `--uac-plugin-dir`. Tests stay behind.
$pluginSource = Join-Path $CoreDir "tooling\opencode-uac-plugin"
$pluginDest = Join-Path $desktop "src-tauri\uac-plugin"
if (-not (Test-Path (Join-Path $pluginSource "index.js"))) { throw "no encuentro el plugin UAC en $pluginSource" }
if (Test-Path $pluginDest) { Remove-Item $pluginDest -Recurse -Force }
New-Item -ItemType Directory -Force -Path $pluginDest | Out-Null
Copy-Item (Join-Path $pluginSource "index.js"), (Join-Path $pluginSource "package.json") $pluginDest -Force
Copy-Item (Join-Path $pluginSource "src") (Join-Path $pluginDest "src") -Recurse -Force

# `cargo build`/`cargo run` do not lay out externalBin: leave a copy next to
# the app binaries so development runs find it too.
# Dev runs (target\<profile>) and `tauri build --target` output (target\<triple>\<profile>).
foreach ($appDir in @((Join-Path $desktop "target\debug"), (Join-Path $desktop "target\release"), (Join-Path $desktop "target\$triple\debug"), (Join-Path $desktop "target\$triple\release"))) {
  if (Test-Path $appDir) {
    Copy-Item $built (Join-Path $appDir "excubia-host-$triple.exe") -Force
    Copy-Item $mcp (Join-Path $appDir "excubia-mcp-$triple.exe") -Force
    Copy-Item $mcp (Join-Path $appDir "excubia-mcp.exe") -Force
    Copy-Item $hook (Join-Path $appDir "excubia-uac-hook.exe") -Force
    Copy-Item $worker (Join-Path $appDir "excubia-media-worker.exe") -Force
  }
}

Write-Output "sidecar: $sidecar"
Write-Output "triple : $triple"
