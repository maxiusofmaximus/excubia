# Genera N aprobaciones de prueba pendientes en el host de pruebas, igual que
# el hook de un agente (excubia-uac-hook -> POST /v1/chat/approvals con token de
# asiento). Nunca aprueba ni deniega: cada aprobación expira sola por su TTL,
# que es la vía normal del host. Ver tooling/emit-alerts.README.md.

param(
    [ValidateRange(1, 500)][int]$Count = 3,
    [ValidateRange(1, 3600)][int]$IntervalSeconds = 5,
    [Parameter(Mandatory = $true)][string]$SeatId,
    # 0 = dos intervalos: así hay 1-2 pendientes a la vez sin denegar nada.
    [ValidateRange(0, 3600)][int]$TtlSeconds = 0,
    [string]$Url = 'http://127.0.0.1:47821',
    [string]$DataDir = 'C:\Users\Max\AppData\Local\Temp\claude\D--Documentos-Projects-excubia\3b08cc96-b207-4697-860e-355b01845680\scratchpad\hn-data-r8',
    [string]$HostExe = (Join-Path $PSScriptRoot '..\target\debug\host.exe'),
    [string]$CsvPath = (Join-Path $PSScriptRoot 'emit-alerts.csv'),
    [switch]$Cleanup
)

$ErrorActionPreference = 'Stop'
if ($TtlSeconds -eq 0) { $TtlSeconds = 2 * $IntervalSeconds }

# El host emite el token de asiento; issue_hook_token revoca el anterior de ese
# asiento. Vive solo en memoria: no se imprime ni se guarda.
function Get-SeatToken {
    $output = & $HostExe hook-token --seat $SeatId --data-dir $DataDir
    if ($LASTEXITCODE -ne 0) { throw "host hook-token falló (código $LASTEXITCODE)" }
    $line = $output | Select-String -Pattern '^hook token:\s*(\S+)' | Select-Object -First 1
    if (-not $line) { throw 'el host no devolvió token de asiento' }
    return $line.Matches[0].Groups[1].Value
}

function Invoke-Host {
    param([string]$Method, [string]$Path, $Body)
    $request = @{
        Method      = $Method
        Uri         = "$Url$Path"
        Headers     = @{ 'X-HN-Hook-Token' = $script:Token }
        ContentType = 'application/json'
        TimeoutSec  = 10
    }
    if ($Body) { $request.Body = $Body | ConvertTo-Json -Depth 5 }
    return Invoke-RestMethod @request
}

function New-ApprovalBody {
    param([int]$Index)
    # El digest identifica la acción: uno distinto por aprobación, o el host
    # rechaza la repetida (approval_digest_reused).
    $nonce = [guid]::NewGuid().ToString('N')
    $hash = [System.Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes("emit-alerts|$nonce"))
    return @{
        kind             = 'command'
        tool             = 'Bash'
        summary          = ('Prueba de avisos {0:D2}' -f $Index)
        action_digest    = 'sha256:' + (-join ($hash | ForEach-Object { $_.ToString('x2') }))
        cwd              = $PSScriptRoot
        paths            = @()
        payload_redacted = @{ prueba = 'emit-alerts' }
        ttl_seconds      = $TtlSeconds
    }
}

# Un 409 aquí es el límite de pendientes: se espera a que expire una y se reintenta.
function Add-Approval {
    param([int]$Index)
    $deadline = (Get-Date).AddSeconds($TtlSeconds + 10)
    while ($true) {
        try {
            return (Invoke-Host -Method Post -Path '/v1/chat/approvals' -Body (New-ApprovalBody $Index)).approval_id
        } catch {
            $code = [int]$_.Exception.Response.StatusCode
            if ($code -ne 409 -or (Get-Date) -ge $deadline) { throw }
            Start-Sleep -Seconds 1
        }
    }
}

function Add-Row {
    param([string]$ApprovalId)
    if (-not (Test-Path $CsvPath)) { 'timestamp,approval_id' | Set-Content -Path $CsvPath -Encoding utf8 }
    $stamp = (Get-Date).ToString('yyyy-MM-ddTHH:mm:ss.fffzzz')
    "$stamp,$ApprovalId" | Add-Content -Path $CsvPath -Encoding utf8
}

# Estado actual de las aprobaciones del CSV. No deniega: las pendientes expiran solas.
function Show-Status {
    if (-not (Test-Path $CsvPath)) { throw "no hay CSV: $CsvPath" }
    $totals = @{}
    foreach ($row in Import-Csv $CsvPath) {
        $status = (Invoke-Host -Method Get -Path "/v1/chat/approvals/$($row.approval_id)").status
        $totals[$status] = 1 + [int]$totals[$status]
    }
    $totals.GetEnumerator() | ForEach-Object { Write-Host "$($_.Key): $($_.Value)" }
}

try { Invoke-RestMethod -Uri "$Url/v1/health" -TimeoutSec 5 | Out-Null } catch { throw "el host no responde en $Url" }
$script:Token = Get-SeatToken

if ($Cleanup) { Show-Status; return }

for ($i = 1; $i -le $Count; $i++) {
    $id = Add-Approval $i
    $status = (Invoke-Host -Method Get -Path "/v1/chat/approvals/$id").status
    if ($status -ne 'pending') { Write-Warning "la aprobación $i no quedó pendiente ($status): el host la resolvió sola" }
    Add-Row $id
    Write-Host "aprobación $i/$Count creada"
    if ($i -lt $Count) { Start-Sleep -Seconds $IntervalSeconds }
}
Write-Host "listo: las pendientes expiran solas en $TtlSeconds s; comprueba con -Cleanup"
