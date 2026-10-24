# RFC — Aprobaciones escaladas (política → scorer opcional → lead → CTO → usuario)

**Estado:** aprobado por el propietario del producto (5 decisiones), 2026-10-22; añadido el nivel 1.5 opcional
**Alcance:** solo documentación; no cambia código, contratos ni datos. Las listas de §7 son el trabajo de CORE9.
**Base:** `contracts/{api,auth,errors,state,pairing}.md`.
**Decisión del propietario del producto:** modo delegado por defecto (política → lead → CTO → usuario), lista de nunca delegar, interruptor «pedirme siempre a mí» y tarjeta compacta en el chat en lugar del modal.
**Decisiones cerradas (2026-10-22):** TTL 120 s por defecto/300 s máximo con 40 s por nivel agente; `always_user` también para lecturas/tests; la petición del lead la decide el CTO y la del CTO el usuario; cambiar el modo exige `APPROVE` (no basta `ADMIN`); reglas de `approvals.toml` en solo lectura en el MVP.

## 1. Niveles y estados

| Nivel | Quién decide | Alcance | Presupuesto de tiempo |
|---|---|---|---|
| `policy` | Host, reglas deterministas | Acciones evidentes y seguras dentro del proyecto | Sincrónico (0 s) |
| `scorer` | Puntuador de riesgo, si está activo | Solo `low` de categorías `auto_ok_if_low`; el resto escala | ≤200 ms (fail-closed) |
| `lead` | Lead del proyecto | Peticiones de su equipo y su proyecto | 40 s |
| `cto` | Alfred | Lo que cruza proyectos o consume recursos compartidos | 40 s |
| `user` | Dispositivo con `APPROVE` | Lista de nunca delegar, falta de información del usuario, solicitud del propio CTO | Resto del TTL total |

**Campos nuevos de `approval`:** `level` (`policy|scorer|lead|cto|user`), `escalated_from` (`policy|scorer|lead|cto|null`), `decider_kind` (`policy|scorer|agent|user`), `decider_id` (id de regla, `ag_…`, `dev_…` o `scorer`), `decided_at`, `reason` (enum de §4), `question` (solo nivel usuario por falta de información), `risk_score` y `scorer_version` (solo si puntuó). `expires_at` sigue siendo el TTL total, fijado por el host.

**Máquina de estados** (el estado central no cambia; la escalada es una transición auditada que mantiene `pending`):

```text
pending(policy) ─allow→ approved → consumed
pending(policy) ─deny─→ denied
pending(policy) ─sin regla / ambiguo / riesgo alto─→ pending(scorer)  escalated_from=policy
pending(scorer) ─low + auto_ok_if_low─→ approved → consumed
pending(scorer) ─medium/high / no auto_ok / timeout / error─→ pending(lead)  escalated_from=scorer
pending(lead)   ─allow→ approved → consumed
pending(lead)   ─deny─→ denied
pending(lead)   ─cruce de proyecto / sin ámbito / solicitud propia / timeout─→ pending(cto)  escalated_from=lead
pending(cto)    ─allow→ approved → consumed
pending(cto)    ─deny─→ denied
pending(cto)    ─nunca delegar / falta información / solicitud propia / timeout─→ pending(user)  escalated_from=cto
pending(*)      ─TTL total vencido─→ expired
approved        ─consume─→ consumed  (single-use; el hook consume justo antes de ejecutar)
```

Reglas de la máquina: la escalada es **monótona** (solo sube; máximo 4 saltos con el scorer activo, 3 sin él), cada nivel decide una sola vez y una aprobación nunca vuelve a un nivel anterior. Un timeout de nivel **escala**, no aprueba ni deniega; en `user` el timeout es `expired` y el hook deniega. `policy` no espera: su decisión es inmediata y no consume TTL. El `scorer` solo existe si su backend no es `none`; con `none` se salta y la petición va al lead. `scorer` nunca ve `never_delegate` (lo resuelve antes el host), no cuenta como aprobación humana y su fallo o lentitud (>200 ms) se ignora y se sigue al lead. El TTL total es 120 s por defecto y 300 s máximo (límite del hook actual); el scorer consume como máximo 200 ms; si al escalar quedan menos de 15 s, se salta directo a `user`; si ya está en `user`, expira.

## 2. Política del host (nivel 1)

**Formato y ubicación:** `approvals.toml` en el directorio de datos del host (junto a `agents.toml` y `devices.json`), propiedad exclusiva del host; ningún agente ni cliente de chat lo escribe. Se recarga por mtime, igual que `agents.toml`, sin reiniciar. No va a `chat.db`: es configuración del operador, no historial.

```toml
version = 1
mode = "delegated"                 # delegated | always_user (global)

[[rule]]
id = "tests-del-proyecto"
decision = "allow"                 # allow | deny
category = ["test", "build"]
tools = ["bash"]
cwd_under = "project"              # project | workspace | any
command_matches = "^(cargo (test|check|clippy)|pnpm (test|run build)|npm test)\\b"

[[rule]]
id = "leer-dentro-del-proyecto"
decision = "allow"
category = ["read"]
cwd_under = "project"

[[rule]]
id = "git-destructivo"
decision = "deny"
category = ["vcs"]
command_matches = "^git (reset --hard|clean -fd|checkout -- \\*)"
```

**Orden de evaluación** (lo aplica el host, nunca el agente):
1. Canonicalizar la acción con las mismas reglas del digest actual (`kind`, `tool`, `args`, `cwd`, `paths`); rutas resueltas, symlinks incluidos.
2. Si la categoría o la ruta cae en `never_delegate`, **salta a `user`** sin evaluar reglas; ninguna regla puede anularlo.
3. Primera regla que coincide gana (`deny` o `allow`); el orden del fichero es la prioridad.
4. Sin coincidencia, herramienta desconocida o payload incompleto: **escala al scorer** (nivel 1.5) si está activo; con `backend=none` va directo al lead. Nunca se permite por defecto.
5. La política solo mira el payload estructurado; el texto libre del agente no participa.

**Defaults conservadores si no hay fichero:** permitir `read`, `test` y `build` con `cwd` dentro del proyecto; denegar `git reset --hard`/`git clean -fd`; `git push` cae en `publish` y va al usuario; todo lo demás escala al scorer si está activo (con `backend=none`, directo al lead). `edit` dentro del proyecto no se permite por defecto en el MVP: lo decide el lead (una regla explícita o `auto_ok_if_low` pueden permitirlo).

**Categorías y nunca delegar:** `read`, `edit`, `test`, `build`, `command`, `network`, `vcs`, `credentials`, `money`, `delete_user_data`, `publish`, `outside_workspace`. Son `never_delegate` (siempre `user`): `money` (gastar dinero real), `delete_user_data` (borrar datos del usuario), `publish` (push/publicar), `credentials` (tocar credenciales o claves) y `outside_workspace` (salir de `workspaces_root`). Las reglas `allow` no pueden cubrir estas categorías; una regla `deny` sí puede endurecer cualquier otra.

**Riesgo:** el host calcula `risk` (`low|medium|high`). `high` nunca se permite en `policy`; como mínimo va al lead, y si toca una categoría `never_delegate` va al usuario.

## 2.5 Nivel 1.5 — Puntuador de riesgo (opcional, `none` por defecto)

**Qué es:** un modelo de decisión de una sola pasada que puntúa la acción y devuelve `risk_score` (número calibrado) y la etiqueta `risk` (`low|medium|high`). Se sitúa entre la política y el lead y **solo** se consulta cuando ninguna regla decide; nunca sustituye a la política ni a `never_delegate`. Modelos públicos: Jev (TypeSafe AI, nube y cerrado), Laya (Convai Innovations, Apache 2.0, ModernBERT ~421M, ~33–40 ms por pregunta en una T4, flojo con más de 20 opciones), Kev y Eikos (autohospedables), Clef y Clef-flash (Cloudflare, pesos Apache 2.0, entrenamiento no publicado; ~209 ms de mediana el 27B y ~39 ms flash según Cloudflare; en sus benchmarks superan a Jev en BFCL, BANKING77 y CLINC150, y pierden en When2Call y BRIGHT) y Microsoft-Decision-1 (Foundry/OpenRouter). La librería `jevals` (MIT, Python) usa este patrón como guardrail `tool_call_risk` (permitir/preguntar/denegar, con escalada a humano); tomamos el concepto, no el código. Los benchmarks son autoinformados y no se han reproducido de forma independiente: **[NV]**.

**Alcance:** nunca decide `never_delegate` (el host lo comprueba antes y va a `user`). Solo puede (a) **escalar** al lead o (b) resolver como `approved` las peticiones `low` de categorías marcadas `auto_ok_if_low` en `approvals.toml`; `medium`, `high`, categoría no marcada, error o timeout suben al lead. No emite `denied` por sí solo ni cuenta como aprobación humana.

**Contrato:** interfaz `RiskScorer` en el host, con backends enchufables (`none` por defecto; `local_http` hacia un servidor local; `cloud` desactivado). Entrada: categoría, herramienta, `cwd` canonicalizado y comando **redactado** (sin secretos). Salida: probabilidad calibrada (`risk_score`), etiqueta `risk` y versión del modelo. Todo se audita con `decider_kind="scorer"`, `decider_id="scorer"` (actor fijo), `risk_score` y `scorer_version` (`<modelo>@<revisión de prompt>`, p. ej. `clef-flash@p1`). La decisión de la fila de auditoría es `approved` o `escalated`.

```toml
[scorer]
backend = "none"                 # none | local_http | cloud
endpoint = "http://127.0.0.1:8765"  # base, sin ruta: local_http añade /v1/chat/completions
timeout_ms = 200                 # fail-closed: se ignora y se sigue al lead
auto_ok_if_low = ["read", "test", "build"]
```

**Estado en CORE9-F2:** `cloud` es solo esquema: en esta build no hay cliente TLS y el backend siempre falla cerrado. Solo `local_http` (loopback) funciona. El contrato completo y el protocolo están en [`../contracts/scorer.md`](../contracts/scorer.md).

**Calibración y seguridad:** antes de activarlo, el umbral se calibra con un conjunto propio de acciones etiquetadas (no vale el benchmark del proveedor); fail-closed si el servicio cae o tarda más de 200 ms; solo se puntúa el comando canónico, nunca el texto libre del agente; no cambia el digest ni la identidad de la acción; no puede aprobar `medium/high` ni saltarse `never_delegate`. La calibración del camino (60 acciones, servidor simulado) da precisión 1,000 y recall 0,971 con `low_below = 0.2`; los números son circulares porque las acciones y `classify` se escribieron juntas, y el recall real de Clef-flash es [NV].

**Monorepo:** no se vendoriza ningún proyecto externo; el modelo corre como servicio aparte. De `jevals` solo se toma el concepto (preguntas tipificadas, umbrales calibrados), implementado en el crate del host.

**Backend recomendado al activar:** Clef-flash (o Clef) autohospedado localmente, por su rendimiento en BFCL (llamadas a herramientas, lo más parecido a puntuar permisos) y coste 0; alternativas locales Kev/Laya/Eikos; Clef por Workers AI/OpenRouter y Microsoft-Decision-1 quedan desactivados por defecto (de pago). El interruptor sigue en `none` hasta pasar la calibración.

**Coste/valor:** 0 cuota con backend local; reduce turnos del lead en lo trivial. **[NV]:** acierto real en nuestro dominio (Laya es de clasificación general), latencia en Windows y que Clef-flash quepa y rinda en una RTX 5070 de 12 GB con 63 GB de RAM (tamaño real sin verificar). Fase: CORE9-F (§10).

## 3. Aprobador-agente (lead y CTO)

**Superficie:** herramienta MCP `approval.decide` en el shim del asiento, con `{approval_id, decision:"approve"|"deny", reason?}`. No es un permiso de dispositivo y no entra en la matriz de `auth.md`: es una capacidad interna (`APPROVE_DELEGATED`) que el host deriva del **rol del asiento y su proyecto**, no del texto de la llamada. El token MCP ya es por run; el host lo sella al asiento.

| Quién | Puede decidir | No puede |
|---|---|---|
| Lead de `prj_…` | `level=lead` de peticiones de asientos **de su** proyecto | Sus propias peticiones (`seat_id`/`run_id` propios), otro proyecto, `never_delegate`, nivel `scorer`/`cto`/`user` |
| Alfred (CTO) | `level=cto` de peticiones de la empresa (cualquier proyecto) | Sus propias peticiones, `never_delegate`, nivel `scorer`/`user`, peticiones ya resueltas |
| `policy` | Solo reglas del host | Texto de agente, payload no canónico |

El ámbito del CTO es la empresa: «otro proyecto» limita al lead, no a Alfred. Aun así, Alfred nunca decide sus propias peticiones ni las categorías duras. El CTO decide el cruce de proyectos y el uso de recursos compartidos; el **gasto de dinero real** no se delega: está en `never_delegate` y va al usuario (§2).

**Reglas duras:**
- Un lead **no** puede aprobar sus propias peticiones ni las de otro proyecto; Alfred (CTO) no puede aprobar las suyas. El host compara `seat_id`, `run_id` y `project_id` dentro de la transacción.
- El texto libre de un agente sigue sin aprobar nada: ni un mensaje de chat, ni una frase en su salida, ni una llamada a `chat.reply` cambian el estado. Solo `approval.decide` con un `approval_id` válido y el nivel que le corresponde.
- `approval.decide` no crea ni reabre aprobaciones; solo mueve una `pending` a `approved`/`denied`. El digest no cambia al escalar y sigue siendo de por vida.
- El consumo sigue siendo del hook/plugin (`POST …/consume`), no del aprobador: aprobar no ejecuta; el hook consume justo antes de la acción.
- Cambiar de nivel no reescribe la fila original: cada transición añade una fila a `approval_audit` (append-only) con `decider_kind` (`policy|scorer|agent|user`), `decider_id` (regla, `ag_…`, `dev_…` o `scorer`), `level`, `reason` y `scorer_version` (solo filas del puntuador). La cadena completa queda reconstruible; retención mínima 1 año.

## 4. Escalada

| Desde | Resultado | Siguiente nivel | Disparador |
|---|---|---|---|
| `policy` | `approved` / `denied` | — | Regla que coincide |
| `policy` | `pending` | `scorer` | Sin regla, herramienta desconocida, riesgo alto, `edit` sin regla |
| `scorer` | `approved` | — | `low` + categoría en `auto_ok_if_low` |
| `scorer` | `pending` | `lead` | `medium`/`high`, categoría no marcada, timeout o error (fail-closed) |
| `lead` | `approved` / `denied` | — | Decisión del lead dentro de su ámbito |
| `lead` | `pending` | `cto` | Cruce de proyectos, recurso compartido, fuera de su ámbito, el solicitante es el propio lead, timeout |
| `lead`/`cto` | `pending` | `user` | `never_delegate`, falta de información, el solicitante es el propio CTO, timeout |
| `user` | `approved` / `denied` | — | Dispositivo con `APPROVE` |
| `user` | `expired` | — | TTL total vencido |

**Razones (`reason`):** cada escalada registra por qué salió de la política: `no_rule` (ninguna regla cubre la acción), `unknown_action` (la categoría no se puede clasificar), `high_risk` (el riesgo, pedido o evaluado, es `high`) y `never_delegate` (categoría dura: va a `user`). Las expiraciones las decide el host (`decider_kind=system`).

**Denegar vs escalar:** una regla `deny` explícita deniega y no se apela en el MVP; la denegación de un lead/CTO es final para esa acción (el operador puede pedir otra acción distinta, con otro digest). Se **escala** cuando falta ámbito, cuando hay ambigüedad, cuando el nivel no puede decidir o cuando vence su presupuesto de tiempo. Nunca se escala una denegación para buscar un «sí».

**Falta de información:** si el lead/CTO no puede decidir porque necesita un dato del usuario, escala a `user` con `reason=missing_information` y `question` concreta y acotada (≤200 caracteres, sin secretos). La tarjeta muestra esa pregunta; el usuario no recibe un volcado del contexto del agente.

**Timeouts y ciclos:** con `scorer` activo, su timeout (>200 ms) es fail-closed y pasa al lead; el timeout de `lead`/`cto` escala con `reason=timeout`; el de `user` expira. La escalada solo sube, dura como máximo 4 saltos con scorer (3 sin él) y no puede volver atrás, así que no hay ciclos posibles. Si un nivel intermedio no existe (scorer `none`, proyecto sin lead, Alfred parado), se salta con `decider_unavailable`; si no queda ningún nivel agente, va al usuario. El reinicio del host marca las aprobaciones `pending` como `expired` y las audita, igual que hoy.

**Qué ve el solicitante:** exactamente lo de hoy. El hook/plugin sigue bloqueado hasta la decisión final y recibe `approved|denied|expired`; `hn-uac-hook` mantiene su contrato (0 permitido y consumido; 2 denegado, expirado o sin respuesta; 1 solo invocación inválida). El agente no ve en qué nivel se decidió: solo el resultado. La cadena de niveles se muestra al usuario en el timeline y en la auditoría.

## 5. Interruptor y ajustes

```toml
# approvals.toml (mismo fichero de §2)
# Estas dos claves están IGNORADAS hasta CORE9-E: el modo vive en chat.db (ver abajo).
mode = "delegated"                 # delegated | always_user (global)

[projects."prj_01J…"]
mode = "always_user"               # override por proyecto
```

- **Modo efectivo** = override del proyecto si existe; si no, el global. `always_user` salta los niveles 1–3 (incluido el 1.5) también para lo trivial: todo va al usuario, incluidas las lecturas y los tests. `delegated` es el valor por defecto y el que aplica si el fichero no existe.
- **Dónde se guarda (implementado en CORE9-D):** en `chat.db`, tabla `approval_mode` (una fila por ámbito: `global` o el id del proyecto), con su auditoría en `approval_settings_audit`. Así el cambio y su rastro se confirman en la misma transacción. La clave `mode` de `approvals.toml` que este RFC proponía queda **ignorada hasta CORE9-E**: el fichero no tiene `deny_unknown_fields`, así que la clave se descarta sin error y no cambia nada. El cambio se audita con `decider_kind=user`, `decider_id=device_id` (el dispositivo firmado que lo pide), `level=user` y `reason=mode_changed`. El `client_id` no es el decidor: es solo la clave de idempotencia, guardada en `client_id` de la fila de auditoría. Incrementa `chat.revision` y emite `chat.approval_settings`.
- **Quién lo cambia:** solo un dispositivo firmado con `APPROVE`, por `POST /v1/chat/approvals/settings` con `{client_id, mode, project_id?}`. `RESPOND` no basta; `ADMIN` no sustituye a `APPROVE` (gestiona dispositivos, no aprobaciones). La lectura (`GET` en la misma ruta) exige `READ_STATE`. Un `client_id` repetido devuelve `replayed=true` y no cambia nada. Ningún agente tiene herramienta para cambiarlo: el MCP no expone escritura de ajustes.
- **Al pasar a `always_user`:** las aprobaciones pendientes de `lead` y `cto` suben a `user` con `reason=mode_changed` y `escalated_from` = el nivel que dejan. Volver a `delegated` no baja las que ya están en `user`.
- **Por qué en el host:** el interruptor decide quién puede aprobar; si viviera en `chat.db` o en un cliente, un agente comprometido podría desactivar la escalada. Es configuración del operador, como `agents.toml`.
- **UI:** Ajustes → Aprobaciones: interruptor global «Pedirme siempre a mí» y una excepción por proyecto. El prototipo debe explicar en una línea que en modo delegado el usuario solo ve lo que la lista de nunca delegar o la falta de información le reserva.

## 6. Coste

- **Política:** 0 turnos de LLM. Las reglas son deterministas y se evalúan en el host; leer, testear o compilar dentro del proyecto no despierta al lead.
- **Lead/CTO:** como máximo **1 turno por aprobación** (o por lote de hasta 5) y contexto corto (resumen, digest, `cwd` etiquetado, resultado de la política; sin transcripción ni salida del panel). Varias aprobaciones pendientes del mismo asiento se **coalescen** en un turno (máx. 5 por prompt); si hay más, se procesan en orden.
- **Límites:** se reutilizan los de CORE7 (12 delegaciones/turno, 60/hora) y se añade un subpresupuesto de aprobaciones: **6 decisiones/hora por asiento** y **20/hora por host**. Agotado el presupuesto, el nivel escala (`budget_exhausted`), nunca se queda en cola indefinida; en `cto` escala al usuario. **Aplazado (no implementado en CORE9-D):** `budget_exhausted` y `missing_information`. El primero necesita el subpresupuesto de aprobaciones; el segundo, el campo `question` de la política. Ninguno cambia el contrato actual.
- **Por qué la política evita despertar al lead en lo trivial:** el bucle normal de un trabajador (leer, correr tests, compilar) se resuelve en `policy`; el lead solo ve lo que cambia archivos, sale del patrón o cruza su ámbito. Métrica objetivo: ≥70 % de aprobaciones resueltas en `policy` (se mide con `level` en la auditoría; tasa real [NV]).

## 7. Cambios de contrato necesarios

| Fichero | Cambio (sin editar en esta tarea) |
|---|---|
| `docs/contracts/api.md` | Filas de `approval` con `project_id`, `level` (`policy`, `scorer`, `lead`, `cto`, `user`), `escalated_from`, `decider_kind/id`, `reason`, `question`, `risk_score`, `scorer_version`; `POST /v1/chat/approvals/settings` (firmada, `APPROVE`) y el frame de ajustes; herramientas MCP `approval.decide` y `approval.pending`; respuesta de `decision` con el nivel; `pending_user_approvals` aditivo en bootstrap/estado |
| `docs/contracts/auth.md` | Sin permiso nuevo de dispositivo: `APPROVE` cubre decisión y ajustes; `APPROVE_DELEGATED` se documenta como capacidad MCP interna, nunca como fila de la matriz; la superficie hook no cambia |
| `docs/contracts/errors.md` | `403 approval_scope_denied` (agente fuera de ámbito), `409 approval_level_mismatch` (nivel que no le toca), `409 approval_not_delegable` (`never_delegate` intentado por agente), `409 approval_self_approval`, `409 approval_budget` (seis decisiones por hora), `422 invalid_mode`, `422 invalid_scorer_config`; el fallo del scorer no es error de cara al usuario (fail-closed, sigue al lead); contrato del hook intacto (0/2/1) |
| `docs/contracts/state.md` | `chat.approval` gana `level`, `escalated_from`, `decider_kind` (incluye `scorer`), `risk_score`, `scorer_version` y `change: created\|escalated\|decided\|consumed`; `pending_approvals` sigue contando todo; `pending_user_approvals` es el nuevo contador de la bandeja del usuario; la entrada de timeline `kind=approval` gana la línea de decisión |
| `docs/contracts/pairing.md` | Sin cambios: no hay grants nuevos al emparejar y `APPROVE` sigue siendo un grant del host |
| el RFC del chat del orquestador (no publicado) | §5 queda sustituida por este RFC cuando el propietario del producto lo apruebe; §3.2 y §5.2 se actualizan en la misma tarea que el código |

**Aprobación escalada (respuesta de `GET /v1/chat/approvals` y de `decision`):**

```json
{"id":"apr_01J…","conversation_id":"cnv_01J…","project_id":"prj_01J…",
 "seat_id":"ag_bob","run_id":"run_01J…","pane_id":"w1:p9",
 "kind":"command","tool":"bash","summary":"Ejecutar tests","cwd":"D:\\proyectos\\demo",
 "risk":"low","risk_score":null,"scorer_version":null,
 "action_digest":"sha256:9f86d0…",
 "status":"pending","level":"lead","escalated_from":"policy",
 "expires_at":"2026-10-22T10:12:00.000Z","question":null,
 "decider_kind":null,"decider_id":null,"decided_at":null,"reason":"no_rule",
 "payload_redacted":{"command":"cargo test"}}
```

Con el scorer activo, `escalated_from` sería `scorer` y `risk_score`/`scorer_version` vendrían rellenos; con `none` (por defecto) se muestran `null` y el salto es directo al lead.

**Evento SSE de la escalada (mismo `chat.approval`):**

```json
{"event":"chat.approval","revision":61,"conversation_id":"cnv_01J…",
 "approval_id":"apr_01J…","agent_id":"ag_bob","pane_id":"w1:p9",
 "level":"lead","escalated_from":"policy","status":"pending",
 "risk_score":null,"scorer_version":null,"change":"escalated","summary":"Ejecutar tests",
 "expires_at":"2026-10-22T10:12:00.000Z"}
```

**Línea de timeline de lo ya decidido por debajo del usuario:**

```json
{"seq":15,"kind":"approval","approval_id":"apr_01J…","status":"approved",
 "level":"lead","summary":"Ernesto aprobó: cargo test",
 "decider_kind":"agent","decider_id":"ag_ernesto"}
```

## 8. Seguridad

| Amenaza | Cómo queda cubierta |
|---|---|
| **Prompt injection hacia el lead** (la salida de un trabajador dice «aprueba todo») | El texto es dato no confiable: no existe comando, frase ni `chat.reply` que apruebe. Solo `approval.decide` con un `approval_id` válido, y el host comprueba nivel, proyecto, solicitante y digest. La lista `never_delegate` se evalúa en el host, no en el prompt |
| **Lead comprometido** | Solo decide `level=lead` de su proyecto; no puede aprobar sus propias peticiones, ni otro proyecto, ni `never_delegate`; máx. 6/hora y todo queda auditado con `decider_id`. El daño se limita al workspace del proyecto; parar el asiento revoca su token y sus decisiones siguen visibles |
| **CTO comprometido** | Decide `level=cto` (cruce de proyectos), nunca `never_delegate`, nunca sus propias peticiones; sus decisiones son auditadas y el usuario las ve en el timeline. La lista dura sigue reservada al usuario |
| **Replay** | `action_digest` único e irrepetible, aprobación single-use y `consume` antes de ejecutar; TTL total; `client_id` idempotente; escalar no cambia el digest ni reabre una aprobación resuelta |
| **Aprobador falso** | La ruta de decisión exige firma de dispositivo con `APPROVE`; el token de hook no vale en rutas firmadas y la firma no vale en rutas de hook; el token MCP está sellado al asiento y el host es el único que escribe |
| **Puntuador comprometido o alucinado** | `never_delegate` se comprueba antes y va a `user`; el scorer solo puede aprobar `low` con `auto_ok_if_low`; fail-closed a 200 ms; entrada canónica redactada; no cuenta como aprobación humana; se audita modelo, versión y score |
| **Elevación de privilegios** | El modo y las reglas solo los cambia un dispositivo con `APPROVE`; ningún agente tiene herramienta de ajustes; `decider_kind`/`decider_id` los pone el host, no el cuerpo |
| **Hook** | `hn-uac-hook` y su contrato no cambian: `0` solo tras `approved` + `consumed`; `2` denegado, expirado o sin respuesta; `1` solo invocación inválida. La aprobación automática de la política pasa por el mismo `approved → consumed`, así que el hook no necesita lógica nueva |

**Riesgo residual:** un CLI que ignore su hook/plugin sigue pudiendo ejecutar sin aprobación (limitación ya documentada en el RFC del chat del orquestador §5.2 (no publicado)); el host no puede impedirlo y la UI lo marca como degradado. La escalada no arregla esa capa.

## 9. UI (texto, sin HTML)

**El usuario solo ve lo que le llega (nivel `user`):**
- **Tarjeta compacta dentro del chat**, en el timeline, no un modal ni una pantalla completa: una línea de título («Bob necesita permiso»), proyecto y agente, comando o herramienta en monoespaciado, `cwd` como etiqueta corta (`demo/…`), chip de riesgo, chip de nivel («te lo pasa Alfred»), digest corto y cuenta atrás del TTL. Si viene por falta de información, la pregunta concreta ocupa el sitio del comando. Botones pequeños **Permitir una vez** / **Denegar**; sin «Permitir siempre».
- **Nada de toast de un toque:** el toast «Permitir» de una pulsación desaparece; aprobar sin ver el comando deja de existir.
- **Línea de timeline para lo resuelto por debajo:** «Política aprobó: `pnpm test`», «Ernesto aprobó: `cargo test`», «Alfred aprobó: tocar el proyecto B». Es una línea, no una tarjeta; al tocarla se abre el detalle con la cadena de niveles y la auditoría. No genera push ni badge.
- **Ajustes → Aprobaciones:** interruptor global «Pedirme siempre a mí» y una excepción por proyecto; texto de ayuda de una línea sobre los niveles; en el MVP las reglas de `approvals.toml` se muestran en solo lectura.

**Qué cambia respecto al diseño anterior:** la hoja UAC anterior era a pantalla completa (pendiente / expirada / sin `APPROVE`); su variante pendiente queda **obsoleta** como pantalla completa y pasa a tarjeta compacta, mientras que sus estados expirado y 403 sin `APPROVE` sobreviven como estados de esa misma tarjeta (el 403 conserva el comando canónico `host devices grant … --permission approve`). Del diseño del chat de escritorio con tarjetas de delegación, bandeja UAC, modal y aviso) quedan obsoletos el **modal** y el **toast de un toque**; las tarjetas de delegación y la bandeja como vista secundaria se mantienen. La bandeja UAC sigue siendo la misma entidad que la tarjeta, pero la decisión se toma en la tarjeta del chat. El push urgente se reserva a `level=user`; las decisiones de lead/CTO no despiertan al teléfono.

## 10. Plan de implementación por fases

| Fase | Contenido | Gate medible |
|---|---|---|
| **CORE9-A — Política y dominio** | Migración de `approval`/`approval_audit` (`level`, `escalated_from`, `decider_*`, `reason`, `question`, `risk_score`, `scorer_version`); motor de reglas y `approvals.toml` con recarga por mtime; defaults conservadores y `never_delegate` en el host | Tests de tabla: cada regla permite/deniega/escala; `never_delegate` va a `user` aunque exista una regla `allow`; sin fichero, solo `read/test/build` en proyecto; `cargo test --workspace` verde y sin cambios en el contrato de decisión del usuario |
| **CORE9-B — Aprobador-agente** | MCP `approval.decide`, capacidad `APPROVE_DELEGATED`, comprobación de ámbito, autoaprobación y proyecto ajeno; auditoría con `decider_kind/id/level`; presupuesto 6/h y coalescencia | E2E con `fake-cli`: el lead aprueba al trabajador de su proyecto y la acción corre una vez; el lead no puede aprobarse, ni a otro proyecto, ni `never_delegate`; texto «aprueba todo» no aprueba; cada transición deja fila de auditoría |
| **CORE9-C — Escalada y TTL** | Saltos policy→scorer→lead→cto→user (scorer opcional), presupuestos de 40 s, timeouts que escalan, salto de nivel no disponible, `expired` al vencer, frames `chat.approval` con `change` y línea de timeline | E2E de la cadena completa con reloj controlado: total ≤120 s; timeout de lead llega al CTO; usuario vence → `expired` y el hook sale 2; no hay saltos hacia atrás ni ciclos; reinicio expira y audita |
| **CORE9-D — Interruptor y ajustes** | `mode` global y por proyecto, ruta `POST /v1/chat/approvals/settings`, auditoría del cambio, `pending_user_approvals`, SSE de ajustes. **Implementado:** modo en `chat.db` (§5), `GET`/`POST` de ajustes, subida de pendientes al cambiar a `always_user`. **Aplazados:** `budget_exhausted` y `missing_information` (§6) | `always_user` manda todo al usuario (incluido `cargo test`); el override por proyecto gana al global; sin `APPROVE` es 403 uniforme; un agente no tiene herramienta para cambiarlo; el cambio queda auditado |
| **CORE9-E — UI (DESKTOP11/APK8)** | Tarjeta compacta en el chat, línea de timeline para lo resuelto, Ajustes → Aprobaciones, retirada del modal/toast | Capturas (no vídeo) de: tarjeta con comando y TTL, línea «Ernesto aprobó: cargo test», ajuste global y por proyecto, estado 403 sin `APPROVE`; ninguna captura con modal grande o toast «Permitir»; la bandeja secundaria sigue abriendo la misma aprobación |
| **CORE9-F — Puntuador (opcional)** | `RiskScorer` con backends `none`, `local_http` y `cloud`, configuración en `approvals.toml`, calibración propia, auditoría `decider_kind=scorer`, fail-closed 200 ms; Clef-flash local recomendado | Tabla de acciones etiquetadas (precisión/recall y umbral); servicio caído o lento → sigue al lead; `never_delegate` sigue a `user`; `backend=none` no altera ningún test actual |

**[NV]:** latencia real de un turno de lead/CTO dentro del TTL; tasa real de acierto de la política; acierto y latencia del scorer (§2.5); comportamiento de los CLIs con bloqueos más largos (el máximo de 300 s ya es el del hook actual); push móvil solo para `level=user`; cableado real de los hooks de Claude/Codex, que sigue [NV] de CORE8; y coste real de los turnos de aprobación (los gates se validarán con reloj controlado y `fake-cli`, coste cero, no con CLIs reales).

FIN
