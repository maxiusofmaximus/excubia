# Installs the newest Excubia desktop release for the current user:
#
#   irm https://raw.githubusercontent.com/maxiusofmaximus/excubia/main/desktop/scripts/install.ps1 | iex
#
# It always fetches the newest `desktop-v*` release and checks the installer
# against the SHA-256 GitHub records for the asset.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$repo = 'maxiusofmaximus/excubia'

$releases = Invoke-RestMethod "https://api.github.com/repos/$repo/releases?per_page=30" -Headers @{ 'User-Agent' = 'excubia-install' }
$release = $releases | Where-Object { $_.tag_name -like 'desktop-v*' -and -not $_.draft -and -not $_.prerelease } | Select-Object -First 1
if (-not $release) { throw 'No encontré ninguna versión de escritorio publicada.' }
$asset = $release.assets | Where-Object { $_.name -like '*-setup.exe' } | Select-Object -First 1
if (-not $asset) { throw "La versión $($release.tag_name) no trae instalador." }
if ($asset.digest -notlike 'sha256:*') { throw 'GitHub no dio la huella del instalador; no instalo sin comprobarla.' }

$file = Join-Path $env:TEMP $asset.name
Write-Host "Descargando $($asset.name) ($($release.tag_name))..."
Invoke-WebRequest $asset.browser_download_url -OutFile $file
$hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
if ("sha256:$hash" -ne $asset.digest) {
  Remove-Item $file
  throw 'La huella del instalador no coincide; descarga descartada.'
}

Write-Host 'Instalando...'
$code = (Start-Process $file -ArgumentList '/S' -Wait -PassThru).ExitCode
Remove-Item $file
if ($code) { throw "El instalador terminó con código $code." }
Write-Host "Excubia $($release.tag_name -replace '^desktop-v') instalado. Ábrelo desde el menú Inicio."
