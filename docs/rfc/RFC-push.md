# RFC — Notificaciones push sin cuenta Google ni compilación (UnifiedPush en capas)

**Estado:** aprobada con condiciones; F1 medida en un TECNO KI7 (HiOS), ver §8.1 · F2 pendiente
**Alcance:** solo documento. No cambia código, contratos ni datos. Los cambios de §5 son trabajo posterior.
**Base:** `docs/contracts/push.md`, `docs/contracts/{api,pairing,state}.md`, `docs/PARITY-GATEWAY.md` (filas de notificaciones) y, solo para leer el comportamiento, el gateway Python antiguo: `gateway/src/herdr_notify/notify/{policy,fcm,dpapi}.py`. No se leyeron credenciales.
**Marcas:** [V] verificado en la fuente citada · [I] inferencia nuestra · [NV] no verificado. Fuentes S1–S10 en §9.

## 0. Resumen

- **Decisión:** UnifiedPush (UP) **en capas**: (1) SSE mientras la app está viva; (2) con Google Play Services, distribuidor gCompat-UP (Google Web Push vía UP); (3) sin Google Play Services, distribuidor ntfy. Sin relay propio ni proyecto Firebase del proyecto.
- El aviso viaja cifrado extremo a extremo (RFC 8291) y solo dice «hay algo, consulta». El móvil consulta `GET /v1/state` y firma antes de actuar.
- **Medido en F1 (un móvil, una ronda):** gCompat-UP 20/20 en ≤4 s con pantalla apagada y app cerrada. ntfy: 0/20 con valores de fábrica, 12/20 con los ajustes de batería, **20/20 en ≤3 s si además se bloquea en la vista de recientes** (paso manual, a repetir tras reiniciar) (§8.1).
- Hay un coste de privacidad asumido: en la capa 2 Google (FCM) y un relay de UnifiedPush ven token, hora y tamaño del aviso, nunca su contenido (§4).

## 1. Opciones

| Opción | Coste real 2026 | Pros | Contras | Veredicto |
|---|---|---|---|---|
| **A. UnifiedPush + distribuidor ntfy** | 0 con ntfy.sh público [NV límites] | Estándar abierto; sin Google; cifrado de extremo a extremo; el host solo necesita salida HTTPS | Hay que instalar un distribuidor (2.ª app); **en F1 solo fue fiable en HiOS con la app bloqueada en recientes** (§8.1) | **Capa 3**: solo sin Google Play Services |
| **A′. UnifiedPush + gCompat-UP (Google Web Push)** | 0 | 20/20 en ≤4 s en F1; mismo cifrado y mismo contrato que A | Depende de Google Play Services; Google y el relay de UnifiedPush ven metadatos (§4) | **Capa 2**: por defecto con Google Play Services |
| B. ntfy directo (cliente propio / SDK) | 0 | Sin distribuidor | Fuera de ntfy.sh, la entrega en tiempo real exige servicio en primer plano con notificación permanente; sin él, retrasos de minutos u horas [V S6] | Descartada como primaria; es D con otro nombre |
| C. Relay propio que envía por FCM | 0 económico (FCM gratis [V S8]; Workers Free: 100.000 peticiones/día, 10 ms CPU [V S10]) | El usuario no crea proyecto Firebase | El mantenedor pasa a guardar la clave de cuenta de servicio y los tokens de todos los usuarios; puede enviar a cualquier dispositivo; responde por abuso | **Descartada** como producto |
| D. Sin push: SSE con servicio en primer plano | 0 | Sin terceros; ya es la ruta primaria del contrato | Requiere alcanzar el PC; app muerta = sin aviso; servicio en primer plano con notificación permanente [I] | **Primaria mientras la app está viva**; no basta sola |
| E. Web Push desde PWA | 0 | Estándar | Solo navegador; en Chrome el endpoint es de Google [V S5 — ejemplo fcm.googleapis.com] | Ruta alternativa de navegador, no base |

Variantes descartadas: UP con distribuidor embebido de FCM [V S4] = C con la APK atada a Firebase del mantenedor.

### 1.1 Notas de A (la recomendada)

- Flujo: la app pide un distribuidor instalado un registro (intent REGISTER con token de conexión y clave VAPID P-256 de 87 bytes en base64url); el distribuidor devuelve un endpoint de hasta 1000 bytes que es una URL de capacidad [V S3].
- El host publica en ese endpoint con Web Push (RFC 8030) cifrando con RFC 8291 usando la clave pública y el secreto de autenticación del registro (`pubKeySet`) [V S4, S9].
- Distribuidores Android: ntfy, NextPush, Sunup, Conversations (proxy de reescritura), y el distribuidor embebido de FCM [V S1].
- Paquete Flutter: `unifiedpush` 6.2.0, licencia Apache-2.0, editor verificado, repositorio en Codeberg, depende de `unifiedpush_android ^3.4.0`; plataformas Android y Linux, sin iOS [V S4; iOS ausente = I].

## 2. Experiencia de usuario nuevo

Sin cuenta Google, sin proyecto Firebase, sin compilar:

1. Instalar la app del PC y la APK del móvil.
2. Instalar un distribuidor UP. **Con Google Play Services: gCompat-UP** (abrirlo una vez y conceder notificaciones). **Sin ellos: la app ntfy desde F-Droid o el APK de GitHub, no la de Google Play**, porque la de Play usa Firebase para el host ntfy.sh y la de F-Droid no [V S6]. El servidor ntfy.sh es abierto por defecto [V S5]; no hay registro de usuario [I].
3. Abrir la app: con un único distribuidor se elige solo («instalación en un paso») [V S6]; la app recibe el endpoint.
4. Emparejar con el PC como hoy. El endpoint viaja en la petición firmada de emparejamiento (§5).
5. Ajustes del fabricante TECNO/HiOS, para la app y para el distribuidor: batería «Sin restricciones» e inicio automático «Permitido» (los nombres de S7 «Power Saving Management For Apps» y «Screen off push block» no aparecen en Ajustes en español [NV]). La app debe enlazar estos ajustes en su pantalla de primer uso.
   - **Coste UX medido (F1):** con gCompat-UP bastan esos ajustes. Con ntfy **no bastan**: aun con ellos el sistema cerró ntfy con la pantalla apagada y 8 de 20 avisos no llegaron. Con ntfy y la app **bloqueadas en recientes** (▼ junto al nombre de la app → «Bloquear») llegaron 20/20. Texto para el usuario sin Google Play Services: «Para recibir avisos con la pantalla apagada, bloquea ntfy y esta app en la vista de recientes; si reinicias el móvil, vuelve a bloquearlas.» [NV: el bloqueo tras reinicio no se midió]

**Cambiar de móvil:** el móvil nuevo repite 1–4 (endpoint nuevo). El endpoint del móvil anterior se borra al revocar el dispositivo, igual que `fcm_token` hoy [V docs/contracts/push.md]. No hay cuenta que recuperar.

**Cambiar de PC:** el móvil conserva su endpoint y lo reenvía al primer contacto con el PC nuevo, sin reinstalar nada [I]. Si se migra `devices.json`, el endpoint viaja con el registro del dispositivo [I].

**Qué ve el usuario sin distribuidor:** un aviso en la app («sin distribuidor: avisos solo con la app abierta») y el enlace para instalar ntfy [I].

## 3. Servidor

- **ntfy.sh público:** gratis. Límites por defecto del servidor ntfy: 60 peticiones de ráfaga por visitante, reposición de una cada 5 s [V S5]. Los límites reales del servicio público y la política de retención no se han verificado [NV]. Con nuestro presupuesto (6/agente/h, 60/h, 400/día) no se alcanza el tope de ráfaga salvo con brotes [I].
- **Privacidad de ntfy.sh:** el nombre del topic es el secreto. Por defecto cualquiera puede leer y escribir un topic que conozca [V S5]. ntfy.sh ve la IP del host y la frecuencia de avisos [I].
- **Si ntfy.sh desaparece o cambia sus límites:** el core envía a la URL del endpoint, no a un servidor fijo; basta que la app registre otro distribuidor (REGISTER/UNREGISTER [V S3]) y reenvíe el endpoint nuevo [I]. El riesgo queda acotado a «endpoint caducado».
- **Contingencia ntfy.sh (abstracción obligatoria):** el core no conoce ntfy. Guarda por dispositivo `{endpoint, p256dh, auth, distributor}` y envía Web Push a esa URL, sea de ntfy.sh, de una instancia propia, de gCompat-UP o de otro distribuidor. En el cliente, el distribuidor es intercambiable: registro, desregistro y endpoint nuevo reenviado por `POST /v1/me/push-endpoint`. Migrar de distribuidor o de servidor es cambiar el endpoint, sin tocar el cifrado, VAPID ni la política. Cambiar de distribuidor en un móvil ya emparejado sin reinstalar [NV: no probado en F1; F2 debe probarlo].
- **ntfy autoalojado en el PC:** para que el móvil lo alcance fuera de la LAN sin Tailscale hace falta IP pública o redirección de puertos (la IP doméstica suele cambiar) o un túnel [NV: túnel gratuito no verificado]. Un VPS cuesta dinero y rompe el presupuesto cero [I]. Por eso: opción, no defecto.
- **ACL en ntfy autoalojado:** UP exige escritura anónima en topics `up*` [V S5]; la ACL no protege frente a publicadores. La protección es VAPID (§4).
- **Otros servidores UP:** NextPush (Nextcloud), Sunup (autopush-rs) y Conversations [V S1]. Útiles si el usuario ya los tiene.

## 4. Seguridad y privacidad

- **El endpoint es una capacidad:** quien lo conozca puede publicar en él [V S3, «capability URLs»]. El distribuidor genera el identificador; la spec pide entropía suficiente y sugiere UUIDv4 para el token de conexión [V S3]. Entropía del topic que genera ntfy [NV].
- **Quién puede publicar:** cualquiera con la URL. Dos barreras:
  - **Cifrado RFC 8291:** la app descifra y, si no puede, entrega el mensaje sin descifrar y marca `decrypted=false` [V S4]. El móvil descarta lo que no descifra [I].
  - **VAPID (RFC 8292):** el host firma cada envío con una clave privada P-256 guardada en el Administrador de credenciales de Windows, mismo patrón que `dpapi.py` [I]. La clave pública VAPID va en el registro [V S3]. Algunos distribuidores lo exigen y otros no [V S4]; si ntfy lo exige [NV].
- **Cifrado extremo a extremo del aviso: viable con UP.** El host cifra con la clave pública del móvil; ntfy y el distribuidor solo mueven bytes [V S4, S9 §3.4]. ntfy no añade cifrado propio [I].
- **Contenido mínimo:** el aviso no lleva texto de agentes, comandos ni `pane_id` en claro [I]. El móvil consulta `GET /v1/state` y firma la acción [V docs/contracts/push.md, «Notify, do not authorize»].
- **Metadatos:** con ntfy, ntfy.sh ve IP del host, topic y momento; el distribuidor, momento de entrega; Google, nada. La app de Play de ntfy envía a Firebase los avisos de ntfy.sh [V S6].
- **Metadatos con gCompat-UP (capa 2):** el endpoint registrado fue `fcm.distributor.unifiedpush.org/wpfcm?t=…`, un relay de UnifiedPush que reenvía a FCM [V F1]. El relay ve endpoint, hora y tamaño; Google (FCM) ve token del móvil, hora y tamaño; ninguno ve el contenido, cifrado con una clave que solo tiene la app [I: no se inspeccionó el payload de FCM]. Operador y retención del relay [NV].
- **Spam y batería:** el spam no descifrable se descarta sin notificación, pero consume batería y cuota [I]. Mitigación: límite de despertares por hora en la app [I].
- **ACK:** el distribuidor debe impedir que una app confirme mensajes de otra; el ACK tiene como máximo 100 bytes [V S3].

## 5. Integración con nuestro contrato

Cambios propuestos para PUSH2 (no aplicados aquí):

- **Campo nuevo `push_endpoint` en el registro del dispositivo**, sustituye a `fcm_token` (`docs/contracts/push.md`, `api.md`, `state.md` si expone el dispositivo) [I]. Contenido: `endpoint` (≤1000 bytes), `p256dh` y `auth` del `pubKeySet` [V S4], y `distributor` (texto libre, para diagnóstico) [I].
- **Ruta:** `POST /v1/me/push-endpoint` con cuerpo `{ "endpoint": "…", "p256dh": "…", "auth": "…", "distributor": "ntfy" }`, firmada como cualquier petición de dispositivo [I]. Sustituye a `POST /v1/me/fcm-token` (`api.md`, `pairing.md`).
- **Registro en el emparejamiento:** la app guarda el endpoint como pendiente (equivale a `TokenRelay.park` de `push.md`) y lo envía tras emparejar o reconectar; si falla, reintenta [I].
- **Política:** `policy.py` es independiente del transporte (`decide(change) → Decision`). Se reutiliza sin cambios: cooldown 120 s, repetición 900 s, 6/agente/h, 60/h, 400/día, horas de silencio donde `URGENT` ignora el silencio y `REVIEW` se aplaza [V L1]. Solo cambia el emisor.
- **Envío:** POST al endpoint con cabecera `TTL` (RFC 8030 la exige [I, verificar]); `Topic` en lugar de `collapse key` para que un aviso nuevo sustituya al anterior [I]; cuerpo cifrado `aes128gcm` según RFC 8291 §3.4; cabecera de autorización VAPID (RFC 8292) [I]. RFC 8291 §5 trae un vector de prueba completo [V S9].
- **Endpoint caducado:** 404 o 410 → borrar `push_endpoint` (equivale a `NOT_FOUND` y `UNREGISTERED` de `fcm.py`, [V L2]). 429 o 5xx → reintento con espera creciente, sin borrar. Un 400 por mensaje mal formado no borra el endpoint (en `fcm.py` se evitó borrar por `INVALID_ARGUMENT` por este motivo [V L2]).
- **Revocación:** revocar el dispositivo borra `push_endpoint`, como hoy `fcm_token` [V docs/contracts/push.md].
- **Cambios mínimos en docs:** `push.md` (de FCM a «push»); `api.md` (ruta nueva); `pairing.md` (campo del dispositivo y paso de emparejamiento); `errors.md` (código para endpoint inválido o demasiado largo); `PARITY-GATEWAY.md` (fila `fcm-token` → `push-endpoint`) [I].

## 6. Flutter/Android

- **Paquete:** `unifiedpush` 6.2.0, Apache-2.0, editor verificado, publicado hace unos 12 meses [V S4]. Mantenimiento moderado [I]. Si se abandona, el protocolo UP está especificado (AND_3.1.0) [V S3] y se puede hablar REGISTER/MESSAGE desde Kotlin nativo [I].
- **Batería TECNO/HiOS:** los ajustes de §2.5 [V S7]. Prueba obligatoria en el TECNO (F1). Si el distribuidor ntfy necesita servicio en primer plano para entregar con la pantalla apagada, la notificación permanente pasa a ser un coste UX a documentar [NV].
- **Sin distribuidor (fallback):** (1) SSE mientras la app está en primer plano o en su servicio; (2) aviso en la app para instalar ntfy; (3) sin distribuidor y con app cerrada, no hay aviso [I]. Aceptamos (3) como límite del modelo sin FCM.
- **iOS:** UP no aparece en las plataformas del paquete [V S4] y no hay vía gratuita para distribuir iOS [I]. Fuera de alcance.

## 7. Migración desde FCM (instalación actual del mantenedor)

- **Coexistencia:** el host envía por FCM y por UP a los dispositivos que tengan ambos. El móvil deduplica por `pane_id` y versión de aviso para que haya una sola entrada en la bandeja [I].
- **Criterio de retirada de FCM:** (a) todos los dispositivos activos tienen `push_endpoint`; (b) 30 días sin entregas solo-FCM; (c) la prueba del TECNO es OK (F1: cumplida con gCompat-UP). Entonces se retiran el envío a FCM, la credencial del Administrador de credenciales, `google-services.json` del APK y la cuenta de servicio [I].
- **El mantenedor** no necesita tocar su proyecto Firebase para UP; sí conserva FCM hasta retirarlo [I].

## 8. Recomendación y plan por fases

- **F0 — esta RFC:** decisiones tomadas (§10).
- **F1 — spike go/no-go en el TECNO (no toca el core).** APK mínima con el paquete `unifiedpush` (o Kotlin nativo) y ntfy desde GitHub; script en el PC que cifra con el vector de RFC 8291 §5 para validar y publica.
  - **Criterio:** 20 de 20 avisos entregados en ≤60 s con pantalla apagada 30 min y app cerrada, con los ajustes de §2.5. Se registra también el resultado sin ajustes.
  - **Go** si ≥19 de 20 con ajustes. Medir 429 de ntfy.sh y si el distribuidor usa servicio en primer plano [NV].
### 8.1 Resultado de F1 (TECNO KI7, HiOS; 1 móvil, 1 ronda por condición)

Criterio Go: ≥19/20 en ≤60 s, pantalla apagada 30 min, app cerrada.

| Cond. | Distribuidor y ajustes | Entregados | Mediana | Máx |
|---|---|---|---|---|
| A | ntfy 1.25.2 (F-Droid); batería «Sin restricciones», inicio automático «Permitido» | 12/20 (1 en ≤60 s) | 703 s | 1364 s |
| B | ntfy; valores de fábrica | 0/20 | — | — |
| D | gCompat-UP 2.1.2; batería «Sin restricciones», inicio automático «Permitido» | **20/20** | **1,05 s** | 3,5 s |
| A2 | ntfy; como A y además ntfy y la app bloqueadas en recientes (inicio automático no verificado tras reinstalar) | **20/20** | **0,77 s** | 2,6 s |

- ntfy.sh devolvió 200 en todos los envíos: el fallo de A y B está en el móvil (el servicio de ntfy murió o no reconectó con la pantalla apagada).
- gCompat-UP depende de Google Play Services; necesita abrirse una vez y conceder notificaciones.
- Cobertura del Go: es un solo modelo de móvil. F2 debe incluir un chequeo en otro dispositivo cuando haya uno [NV].
- A2 sin verificar el inicio automático tras reinstalar: no aísla del todo el bloqueo; la causa más probable es el bloqueo (en A el servicio murió con la pantalla apagada, en A2 no hubo ninguna muerte) [I].
- Límites: un solo móvil y una ronda por condición. Repetir A2 y D otro día antes de fijar la decisión; no medido tras reinicio del móvil.
- La edición Play de ntfy (C) no se midió.

- **F2 — core:** envío en Rust (cifrado 8291 + VAPID + POST), `push_endpoint` en el almacén de confianza, tabla de suscripciones con reintentos, abstracción de distribuidor, `policy.py` portada (ya prevista en `PARITY-GATEWAY.md`).
  - **Criterio:** tests con el vector de RFC 8291 §5; 404/410 poda el endpoint; 429 reintenta; prueba de extremo a extremo PC → ntfy.sh → TECNO.
- **F3 — emparejamiento y ciclo de vida:** reenvío del endpoint, cambio de móvil y de PC, revocación.
  - **Criterio:** los flujos de §2 probados en el TECNO.
- **F4 — opcional:** ntfy autoalojado con túnel (si se verifica gratis) y otros distribuidores.
  - **Criterio:** igual que F1 con ese servidor.
- **F5 — coexistencia y retirada de FCM (§7).**
  - **Criterio:** los tres puntos de §7.

## 9. Fuentes

- S1 https://unifiedpush.org/users/distributors/ — lista de distribuidores y autoalojamiento.
- S2 https://unifiedpush.org/users/distributors/ntfy/ — página del distribuidor ntfy (solo comprobada su existencia) [NV en detalle].
- S3 https://unifiedpush.org/developers/spec/android/ — especificación Android AND_3.1.0.
- S4 https://pub.dev/packages/unifiedpush — paquete Flutter 6.2.0.
- S5 https://docs.ntfy.sh/config/ — configuración de ntfy: límites, ACL, UP, Web Push, FCM.
- S6 https://docs.ntfy.sh/subscribe/phone/ — app de ntfy, Play frente a F-Droid, entrega instantánea, UnifiedPush.
- S7 https://dontkillmyapp.com/tecno — ajustes de batería de TECNO.
- S8 https://firebase.google.com/pricing — FCM sin coste (plan Spark).
- S9 https://datatracker.ietf.org/doc/html/rfc8291 — RFC 8291, cifrado de Web Push.
- S10 https://developers.cloudflare.com/workers/platform/limits/ — límites de Workers Free.
- L1 `gateway/src/herdr_notify/notify/policy.py` — política de notificación (repositorio antiguo).
- L2 `gateway/src/herdr_notify/notify/fcm.py` — envío FCM, poda de tokens.

## 10. Decisiones tomadas

1. UnifiedPush en capas: SSE con la app abierta; gCompat-UP con Google Play Services; ntfy sin ellos.
2. ntfy.sh público por defecto en la capa 3; ntfy autoalojado solo como opción.
3. ntfy desde F-Droid o GitHub, no la edición de Play.
4. Sin relay propio con FCM (C): el proyecto no custodia tokens ajenos.
5. Cifrado RFC 8291 y VAPID obligatorios.
6. SSE en primer plano como fallback.
7. F1 en el TECNO antes de tocar el core (hecha, §8.1).
8. Retirada de FCM: 30 días sin entregas solo-FCM y TECNO OK.
9. iOS fuera de alcance hasta que haya una vía gratuita.
10. Se acepta la dependencia de Google Play Services en la capa 2, con metadatos visibles para Google y el relay (§4); se documenta en la UX.
