# Seeds the OpenCode plugin tree in the host's canary cache with pnpm, so the
# UAC preflight runs offline on its first pass. OpenCode 1.19.3 bundles the
# provider SDK (`@ai-sdk/openai-compatible`); what it installs on first plugin
# load is its own `@opencode-ai/plugin`, with an embedded npm. Seeding it here
# with pnpm removes that network cost. Keep the versions aligned with the CLI
# that the host runs.
param(
    [string] $DataDir = (Join-Path $env:APPDATA "herdr-notify"),
    [string] $OpenCodeVersion = "1.19.3",
    [string] $ProviderVersion = "3.0.75"
)

$ErrorActionPreference = "Stop"
if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    throw "pnpm is required (no npm in this worktree)"
}

$config = Join-Path $DataDir "uac-canary\xdg\config\opencode"
New-Item -ItemType Directory -Force -Path $config | Out-Null

pnpm add --dir $config --ignore-scripts `
    "@opencode-ai/plugin@$OpenCodeVersion" `
    "@ai-sdk/openai-compatible@$ProviderVersion"
if ($LASTEXITCODE -ne 0) {
    throw "pnpm add failed with exit code $LASTEXITCODE"
}
Write-Host "canary cache seeded at $config"
