# emit-alerts: aprobaciones de prueba para medir los avisos del móvil

Uso (host de pruebas ya en marcha en 127.0.0.1:47821):

    pwsh -File tooling/emit-alerts.ps1 -SeatId <id_asiento> -Count 3 -IntervalSeconds 5
    pwsh -File tooling/emit-alerts.ps1 -SeatId <id_asiento> -Cleanup

- Crea `Count` aprobaciones «Prueba de avisos NN» por el endpoint del hook, una cada `IntervalSeconds`.
- Nunca aprueba ni deniega: cada una expira sola a los `TtlSeconds` (por defecto 2 × intervalo, así hay 1-2 pendientes).
- Auth: `host hook-token --seat` emite un token de asiento que solo vive en memoria; revoca el anterior de ese asiento.
- Registro: `emit-alerts.csv` junto al script (marca ISO con ms, approval_id). No lo subas al commit.
- Id del asiento: fila `agent_seat` de `chat.db` con name = 'Alfred' (lectura).
- Sin modelos ni coste; no toca código Rust ni reinicia el host.
