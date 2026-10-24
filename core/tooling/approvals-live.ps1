# Arnés vivo de aprobaciones (TASK-CORE10).
#
# Arranca el host REAL (host.exe) con un directorio de datos aislado y el
# puerto fijo 18878, empareja dos dispositivos de prueba y lanza un agente
# Claude REAL (claude-haiku-5-5) cuyo hook de Bash es el `hn-uac-hook.exe`
# real, apuntando al host. Cada Bash pide una aprobación al operador.
#
#   start   host en 127.0.0.1:18878, dispositivos "operador" (approve) y
#           "observador" (sin approve), modo always_user, token de asiento
#           del agente Alfred (el que usaría un panel residente).
#   status  dirección, código de emparejamiento NUEVO y aprobaciones pendientes.
#   prompt  lanza Claude en segundo plano con un prompt que obliga a ejecutar
#           `echo x >> marker.txt` con Bash: provoca una aprobación pendiente.
#   stop    detiene el host, sus hijos y cualquier proceso que cite el
#           directorio del arnés; borra ese directorio.
#
# Por qué Claude corre desde el arnés y no desde el chat: el asiento de chat
# de Alfred se lanza con `--restricted` (sin Bash, CORE6B). Los paneles
# residentes de CLIs reales siguen sin implementarse ([NV]). Ver CORE10-RESULT.md.
#
# Todo vive en %TEMP%\hn-approvals-live. Nunca toca %APPDATA% ni el estado
# real del host, y nunca detiene procesos ajenos (8787, 8772, fake-agent):
# solo los descendientes de su propio host y los que citan su directorio.
#
# Móvil (Android): `adb reverse tcp:18878 tcp:18878` y emparejar contra
# http://127.0.0.1:18878. Escritorio: http://127.0.0.1:18878 directamente.

param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet('start', 'status', 'prompt', 'stop')]
    [string]$Command
)

$ErrorActionPreference = 'Stop'

$Port = 18878
$Url = "http://127.0.0.1:$Port"
$Model = 'claude-haiku-5-5'
$Root = Join-Path $env:TEMP 'hn-approvals-live'
$Data = Join-Path $Root 'data'
$Work = Join-Path $Root 'work'
$StateFile = Join-Path $Root 'state.json'
$SettingsFile = Join-Path $Root 'claude-settings.json'
$SeatTokenFile = Join-Path $Root 'seat-token.txt'
$Marker = Join-Path $Data 'marker.txt'
$Bin = Join-Path $PSScriptRoot '..\target\debug'
$HostExe = Join-Path $Bin 'host.exe'
$DemoExe = Join-Path $Bin 'demo-client.exe'
$HookExe = Join-Path $Bin 'hn-uac-hook.exe'
$Operator = Join-Path $Root 'operator.json'
$Observer = Join-Path $Root 'observer.json'

# Lee el estado del arranque anterior, o devuelve $null si no hay arnés.
function Read-State {
    if (-not (Test-Path $StateFile)) { return $null }
    return Get-Content $StateFile -Raw | ConvertFrom-Json
}

# Ejecuta demo-client y lanza error si el host contesta un no-2xx.
function Invoke-Demo {
    param([string[]]$Arguments)
    $output = & $DemoExe @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "demo-client $($Arguments[0]) falló: $output" }
    return $output
}

# Devuelve el JSON que demo-client imprime tras su línea `HTTP 200`.
function Read-Json {
    param($Output)
    return (($Output | Select-Object -Skip 1) -join "`n") | ConvertFrom-Json
}

# Emite un código de emparejamiento de un solo uso (válido 300 s).
function New-PairingCode {
    $line = & $HostExe pair-code --data-dir $Data --ttl 300 | Select-String '^pairing code:'
    return ($line.Line -replace '^pairing code:\s*', '').Trim()
}

# Empareja un dispositivo de prueba y devuelve su id. La clave privada queda
# solo en el fichero de identidad; el código de recuperación no se imprime.
function New-Device {
    param([string]$Label, [string]$Identity)
    $code = New-PairingCode
    $response = Read-Json (Invoke-Demo @('pair', '--url', $Url, '--code', $code, '--identity', $Identity, '--label', $Label))
    return $response.device_id
}

# Concede un permiso a un dispositivo mediante el CLI del host; falla si no se aplica.
function Grant-Permission {
    param([string]$DeviceId, [string]$Permission)
    $output = & $HostExe devices grant $DeviceId --permission $Permission --data-dir $Data 2>&1
    if ($LASTEXITCODE -ne 0) { throw "no se pudo conceder ${Permission} a ${DeviceId}: $output" }
}

# Espera a que el host responda en el puerto.
function Wait-Listening {
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline) {
        if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { return }
        Start-Sleep -Milliseconds 300
    }
    throw "el host no escucha en $Port tras 30 s; mira $Root\host.err.log"
}

# Detiene un proceso y todos sus descendientes (primero las hojas).
function Stop-Tree {
    param([int]$ProcessId)
    Get-CimInstance Win32_Process -Filter "ParentProcessId=$ProcessId" |
        ForEach-Object { Stop-Tree -ProcessId $_.ProcessId }
    Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue
}

# Id del asiento de Alfred, leído del bootstrap del chat (el seat del CTO).
function Get-AlfredSeatId {
    $boot = Read-Json (Invoke-Demo @('chat-bootstrap', '--url', $Url, '--identity', $Operator))
    if (-not $boot.alfred.agent_id) { throw 'el bootstrap del chat no trae el asiento de Alfred' }
    return $boot.alfred.agent_id
}

# Ajustes de Claude del arnés: los escribe el propio host (`claude-settings`),
# el mismo código que usa el lanzamiento real. Aquí no hay copia.
function Write-ClaudeSettings {
    & $HostExe claude-settings --hook $HookExe --out $SettingsFile
    if ($LASTEXITCODE -ne 0) { throw "host claude-settings falló (código $LASTEXITCODE)" }
}

# Cuenta las aprobaciones pendientes del operador (0 si el host no contesta).
function Get-PendingCount {
    $pending = Read-Json (Invoke-Demo @('approvals', '--url', $Url, '--identity', $Operator, '--status', 'pending'))
    return @($pending.approvals).Count
}

switch ($Command) {
    'start' {
        if (Read-State) { throw 'ya hay un arnés activo: ejecuta stop primero' }
        if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) {
            throw "el puerto $Port ya está en uso; no se toca ese proceso"
        }
        foreach ($exe in $HostExe, $DemoExe, $HookExe, (Join-Path $Bin 'hn-mcp.exe')) {
            if (-not (Test-Path $exe)) { throw "falta ${exe}: ejecuta cargo build --workspace --bins" }
        }
        New-Item -ItemType Directory -Force -Path $Data, $Work | Out-Null

        $process = Start-Process -FilePath $HostExe -PassThru -WindowStyle Hidden `
            -ArgumentList @('--data-dir', $Data, '--port', "$Port", '--bind', '127.0.0.1', '--label', 'approvals-live') `
            -RedirectStandardOutput (Join-Path $Root 'host.log') `
            -RedirectStandardError (Join-Path $Root 'host.err.log')
        # Se guarda el pid antes de esperar: si algo falla, stop lo sigue encontrando.
        @{ pid = $process.Id } | ConvertTo-Json | Set-Content $StateFile
        Wait-Listening

        $operatorId = New-Device -Label 'operador' -Identity $Operator
        $observerId = New-Device -Label 'observador' -Identity $Observer
        # Permisos mínimos: el observador puede leer y responder, nunca aprobar.
        foreach ($device in $operatorId, $observerId) {
            Grant-Permission -DeviceId $device -Permission 'read_state'
            Grant-Permission -DeviceId $device -Permission 'respond'
        }
        Grant-Permission -DeviceId $operatorId -Permission 'approve'

        # Modo de aprobación: cada acción pasa por el operador (always_user).
        Invoke-Demo @('set-approval-mode', '--url', $Url, '--identity', $Operator, '--client-id', 'live-mode-1', '--mode', 'always_user') | Out-Null

        # Token de asiento (no el del host): cubre las aprobaciones de Alfred.
        $seatId = Get-AlfredSeatId
        $token = (& $HostExe hook-token --seat $seatId --data-dir $Data | Select-String '^hook token:').Line -replace '^hook token:\s*', ''
        Set-Content -NoNewline -Path $SeatTokenFile -Value $token.Trim()

        Write-ClaudeSettings
        @{ pid = $process.Id; operator = $operatorId; observer = $observerId; seat = $seatId } |
            ConvertTo-Json | Set-Content $StateFile
        Write-Host "arnés listo: $Url (host pid $($process.Id), Claude = $Model, modo always_user)"
        Write-Host "operador $operatorId (approve) · observador $observerId (sin approve) · asiento $seatId"
    }
    'status' {
        $state = Read-State
        if (-not $state) { throw 'no hay arnés activo: ejecuta start' }
        if (-not (Get-Process -Id $state.pid -ErrorAction SilentlyContinue)) { throw 'el host del arnés no está vivo' }
        Write-Host "dirección: $Url"
        Write-Host "código de emparejamiento nuevo: $(New-PairingCode)"
        $pending = Read-Json (Invoke-Demo @('approvals', '--url', $Url, '--identity', $Operator, '--status', 'pending'))
        Write-Host "aprobaciones pendientes: $(@($pending.approvals).Count)"
        $pending.approvals | ForEach-Object { Write-Host "  $($_.id)  $($_.tool)  $($_.summary)" }
    }
    'prompt' {
        $state = Read-State
        if (-not $state) { throw 'no hay arnés activo: ejecuta start' }
        $stamp = Get-Date -Format 'yyyyMMddHHmmss'
        # Bash de Git recibe la ruta con barras normales.
        $markerPath = $Marker -replace '\\', '/'
        # El comentario final da un digest distinto a cada turno: el host nunca
        # vuelve a aprobar la misma acción (409 approval_digest_reused).
        $text = "Ejecuta exactamente un comando con la herramienta Bash: echo x >> $markerPath # run-$stamp " +
            '. No uses ninguna otra herramienta. Después responde solo: listo.'
        # El hook recibe el token de asiento y el host real por el entorno.
        $env:HN_HOOK_URL = $Url
        $env:HN_HOOK_TOKEN = (Get-Content $SeatTokenFile -Raw).Trim()
        # El prompt entra por stdin desde un fichero: Start-Process no citaba bien
        # el argumento y Claude recibía solo «Ejecuta».
        $promptFile = Join-Path $Root "prompt-$stamp.txt"
        Set-Content -Path $promptFile -Value $text -NoNewline
        $claudeArgs = @(
            '-p',
            '--model', $Model,
            '--settings', $SettingsFile,
            '--setting-sources', 'project',
            '--tools', 'Bash',
            '--allowedTools', 'Bash',
            '--no-session-persistence',
            '--output-format', 'text'
        )
        $claude = Start-Process -FilePath (Get-Command claude).Source -PassThru -WindowStyle Hidden `
            -WorkingDirectory $Work -ArgumentList $claudeArgs `
            -RedirectStandardInput $promptFile `
            -RedirectStandardOutput (Join-Path $Root "claude-$stamp.out") `
            -RedirectStandardError (Join-Path $Root "claude-$stamp.err")
        Remove-Item Env:HN_HOOK_TOKEN
        Write-Host "claude en marcha (pid $($claude.Id)); la salida va a claude-$stamp.out"
        # Criterio C: la aprobación tiene que verla el operador (bandeja de un
        # cliente emparejado con approve) en 10 s como mucho.
        $started = Get-Date
        $deadline = $started.AddSeconds(10)
        while ((Get-PendingCount) -eq 0 -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
        $seconds = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)
        if ((Get-PendingCount) -eq 0) {
            Write-Host "AVISO: ninguna aprobación pendiente visible tras $seconds s"
        } else {
            Write-Host "aprobación pendiente visible para el operador tras $seconds s (status la lista)"
        }
    }
    'stop' {
        $state = Read-State
        if ($state) { Stop-Tree -ProcessId $state.pid }
        # Procesos que citan este directorio (Claude del arnés y sus hijos).
        Get-CimInstance Win32_Process |
            Where-Object { $_.CommandLine -and $_.CommandLine.Contains($Root) } |
            ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
        if (Test-Path $Root) {
            if ((Split-Path $Root -Leaf) -ne 'hn-approvals-live') { throw "ruta inesperada: $Root" }
            Remove-Item -Recurse -Force $Root
        }
        Write-Host 'arnés detenido y limpio'
    }
}
