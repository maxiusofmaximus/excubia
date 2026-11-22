# RFC — Transportes intercambiables (acceso remoto sin VPN obligatoria)

> Nota (2.0): documento histórico. `add-device` y los enlaces `herdr-notify://` se retiraron en 2.0; hoy se empareja con `host pair-code`, enlaces `excubia://` y verificación SAS (ver `docs/contracts/pairing.md`).

**Estado:** propuesta. Solo documento: no cambia código, contratos ni datos.
**Origen:** decisión del Master, 2026-10-23. Tailscale bloqueó el móvil ~6 h (APK8D); el producto no puede depender de un servicio externo que no controla.
**Marcas:** [V] verificado en la fuente citada · [I] inferencia nuestra · [NV] no verificado (se indica qué medir).

## 0. Resumen

- Hoy el host sirve LAN con TLS 1.3 fijado por SPKI y se empareja por enlace/código. Fuera de la LAN no hay camino propio: se usa Tailscale.
- Propuesta: un trait `Transport` en el núcleo y un orden de intento fijo: **LAN → p2p → VPN opcional**. Túnel solo como último recurso (descartado salvo emergencia).
- **Recomendación:** no adoptar iroh en producción todavía. Sus relés públicos son para desarrollo y uso personal [V]. Un relé propio necesita un host alcanzable desde Internet, lo que choca con el presupuesto cero [I]. Antes, un spike medido (§5, F2).
- Antes de cualquier transporte nuevo, la app debe distinguir «sin ruta» de «host caído» (hoy no lo hace, §1.4).

## 1. Estado actual

### 1.1 Transportes que existen

| Transporte | Dónde está | Identidad del servidor | Identidad del cliente | Estado |
|---|---|---|---|---|
| Loopback HTTP | `crates/host/src/bin/host/daemon.rs:54` | ninguna (solo `127.0.0.1`) | firma por dispositivo | activo |
| LAN TLS 1.3 + pin SPKI | `daemon.rs:66-74`, `:229-247`; `crates/api/src/tls.rs`; `crates/api/src/lib.rs:358` (`serve_client`) | certificado autofirmado P-256; pin = SHA-256 del SPKI en base64url (43 car.) [`crates/pin/src/{identity,fingerprint,tls,verifier}.rs`] | firma de petición por clave de dispositivo | implementado, off salvo `--tls-bind` |
| Emparejamiento LAN | `host pair-code --lan` → enlace `herdr-notify://pair?v=1&c=…&h=…&a=…&e=…` [`docs/contracts/pairing.md:178-208`] | `h=` (pin) dentro del enlace | clave de dispositivo registrada | implementado; el QR lo pinta la app de escritorio/APK |
| Tailscale (auto-aprobación) | `crates/auth/src/tailnet.rs:19,28,34`; `crates/api/src/routes/pair.rs:74` | IP de la tailnet (`tailscale whois`) | IP de origen → dueño de la tailnet | implementado; depende del daemon |
| Tailscale (direcciones) | `docs/troubleshooting-tailscale.md` | — | — | implementado; depende del daemon |
| Peers de la tailnet | `GET /v1/peers` [`docs/contracts/api.md:41`] | — | — | **sin ruta en `crates/`** |

Verificado también: el API ya separa la conectividad de la aplicación: «Reaching it from a phone is the transport's job» [`docs/contracts/api.md:3-4`]. El mapa de la app lo confirma: `AddressProber.firstHealthy` prueba varias direcciones (guardada, nombre de tailnet, IP de tailnet) en paralelo [`android/app/src/main/java/dev/herdrnotify/app/pairing/AddressProber.kt`].

### 1.2 Dónde está la dependencia de la tailnet

- Runtime en el host: `crates/auth/src/tailnet.rs` ejecuta el binario `tailscale` (rutas fijas en Windows y Linux, `tailnet.rs:93-108`). Si falta el binario o el daemon está parado, la respuesta es «sin dueño», y entonces el emparejamiento sin código queda rechazado [`docs/contracts/pairing.md:93-98`].
- Runtime en el móvil: el `PeersDirectory` y la app asumen tailnet para `GET /v1/peers` (`PeersUnavailable`, `PeersDirectory.kt:15`).
- Documentación: `docs/roadmap.md:20` fija M4 «Remote access — Tailscale + TLS, verified off-LAN — not started».
- Grep: 59 ficheros del worktree coinciden con `tailnet|tailscale`. Incluye notas `TASK-*` y `*-RESULT.md`; la dependencia de runtime es la lista anterior.

### 1.3 Identidad por IP (incumple el requisito de clave)

La auto-aprobación identifica al dispositivo por la IP de origen y su dueño de tailnet, no por su clave [`pair.rs:74`]. Es la única ruta de emparejamiento que no ancla en clave de dispositivo. Los códigos y la recuperación sí son por clave.

### 1.4 Errores que ve el usuario (incumple «host caído ≠ sin ruta»)

> Nota: las rutas Kotlin de esta sección son de la app antigua. La app actual es Flutter (`mobile/lib/core/api/host_client.dart`, `HostError` con `status == 0`) y tiene la misma mezcla: cualquier fallo de red o plazo agotado es «Sin conexión con el PC.». [NV: sin revisar el resto de la app Flutter]

- `HostClient.kt:36-49` agrupa todo fallo de red en `HostError.Unreachable`.
- `AddressProber.firstHealthy` devuelve `null` si ninguna dirección responde [`AddressProber.kt`]. «Ninguna responde» incluye «sin red» y «host apagado» por igual.
- `PeersUnavailable` («tailnet unavailable») mezcla «no hay tailnet» con «el host no contesta» [`PeersDirectory.kt:15`].

## 2. Requisitos

- **R1** El cliente distingue **sin ruta** (ningún transporte completó el handshake con la identidad fijada) de **host caído** (un transporte llegó al host o a un relé, y el host responde «offline» o falla el health firmado). Hoy no existe (§1.4).
- **R2** Identidad por clave de dispositivo, nunca por IP. La IP de origen puede ser una pista, no una prueba.
- **R3** TLS fijado en el enlace/QR (ya existe para LAN). Cualquier transporte nuevo lleva su identidad (p. ej. la clave pública del endpoint) dentro del mismo enlace.
- **R4** Sin servicio de pago. Presupuesto cero.
- **R5** Cifrado extremo a extremo si hay un tercero en medio (relé). El tercero ve metadatos, nunca contenido.

## 3. Opciones

| Opción | Qué exige al usuario | Coste | Madurez | Licencia | Encaje Rust / Android-Flutter |
|---|---|---|---|---|---|
| **(a) LAN directa + TLS fijado** | Estar en la misma red; `--tls-bind`; emparejar con el enlace | 0 | Alta: en repo, con pruebas (`crates/host/tests/lan_tls.rs`) | Propia | Ya en `crates/api`, `crates/pin`. La app ya usa cliente con pin (`PairingApi`, `HostClient`) |
| **(b) p2p con relé — iroh** | Un EndpointId en el enlace; con relé público, nada más. Con relé propio, un host alcanzable | Relé público: 0, pero **solo desarrollo y uso personal** [V]. Relé propio: VPS o máquina con IP pública [I], incumple R4 | iroh 1.4.1 en `Cargo.toml` [V]. «1.0 estable el 15/06/2026» viene de un blog de terceros [NV] | `MIT OR Apache-2.0` en `Cargo.toml` [V]; el repo GitHub marca Apache-2.0 [V] | Crate Rust nativo. Sin binding Kotlin verificado; el blog menciona bindings Kotlin/Swift [NV]. Flutter exige FFI o puente nativo [I] |
| **(c) VPN opcional** — Tailscale / Headscale / NetBird | Cuenta (Tailscale) o servidor de control propio (Headscale). Cliente en el móvil y el PC | Tailscale personal: [NV] límites del plan gratuito. Headscale: el control plane debe ser alcanzable [I] | Alta (Tailscale) | Cliente Tailscale **BSD-3-Clause** [V]; Headscale **BSD-3-Clause** [V]; NetBird: GitHub no detecta licencia (`NOASSERTION`) [V] → [NV] revisar LICENSE | Opción de red, no código: el host ya puede escuchar en la IP de la VPN (`--tls-bind`) |
| **(d) túnel — cloudflared / ngrok** | Cuenta del proveedor; dominio o subdominio | cloudflared gratuito [NV] límites; ngrok [NV] | Alta | cloudflared **Apache-2.0** [V]; ngrok propietario [NV] | El borde termina TLS: sin E2E salvo capa propia por encima [I]. Último recurso |

Notas:
- iroh hace agujero NAT primero y cae al relé si falla. El relé solo reenvía paquetes cifrados dirigidos a EndpointIDs; no lee el tráfico [V, `iroh/README.md`].
- Los límites del relé público no están publicados; se afinan según uso. La documentación de n0 dice que no se use en producción [V, `docs.iroh.computer/relays/rate-limiting.md`]. Un relé propio sí puede configurarse sin límite [V, misma fuente].
- Los porcentajes de éxito (≈90 % agujero, ≈95 % directo) vienen de un blog de terceros [NV].

## 4. Interfaz estable propuesta

**Sin implementar nada.** Un crate nuevo `crates/transport` con solo el trait y los tipos; la LAN actual es la primera implementación.

```rust
/// Un camino hacia un par identificado por su clave, nunca por su IP.
pub trait Transport {
    fn kind(&self) -> TransportKind;                    // Lan | P2p | Vpn | Tunnel
    fn connect(&self, peer: &PeerRef) -> impl Future<Output = Result<Route, TransportError>> + Send;
    fn accept(&self) -> impl Future<Output = Result<Incoming, TransportError>> + Send;
    fn route_state(&self) -> RouteState;               // Direct{rtt} | Relayed{rtt} | Down
}

pub struct PeerRef { pub key: PublicKey, pub hints: Vec<Hint> } // clave + pistas (IP, EndpointId, nombre)

pub enum TransportError {
    NoRoute,        // ningún camino completó el handshake con la identidad fijada (R1)
    HostDown,       // un camino llegó y el host responde offline o falla el health firmado (R1)
    IdentityMismatch, // la clave presentada no es la fijada: nunca se reintenta
    Refused,        // el host rechazó al dispositivo (revocado, sin grant)
}
```

- `impl Future` en trait requiere Rust ≥ 1.75 [NV: versión mínima del workspace sin verificar].
- `IdentityMismatch` no se reintenta por otro transporte: un pin fallido es un ataque o un cambio de certificado, no una red caída.

**Orden de intento:** `Lan` → `P2p` → `Vpn` (las pistas de `PeerRef.hints` se reparten por transporte). Cada transporte tiene su tiempo máximo; gana el primero que devuelve `Route` con la identidad fijada. Si todos fallan, se devuelve el error de **mayor especificidad**: `HostDown` si alguno llegó al host; si no, `NoRoute`.

**Mensajes al usuario (español):**
- `NoRoute`: «No llego al PC por ninguna ruta. ¿Está encendido y en la misma red? Si usas la VPN, compruébala.»
- `HostDown`: «El PC responde, pero el servicio está parado. Arráncalo en el PC y vuelve a intentarlo.»
- `IdentityMismatch`: «El PC ya no es el que emparejaste. Vuelve a emparejarlo desde el PC.»
- `Refused`: «Este móvil ya no tiene acceso a ese PC. Pide un nuevo emparejamiento.»

## 5. Plan por fases

| Fase | Qué se hace | Criterio de aceptación | No se hace aún |
|---|---|---|---|
| **F0** Errores honestos | La app separa `NoRoute` de `HostDown` (§1.4). Sin cambios de red | Test: direcciones inalcanzables → `NoRoute`; host con health 503 → `HostDown`; ninguna dirección cambia de `null` a un tipo distinto sin que el test lo cubra | Transportes nuevos |
| **F1** Trait + LAN | `crates/transport` con el trait; LAN como única implementación; orden y mensajes de §4 | `crates/host/tests/lan_tls.rs` sigue verde; `cargo clippy -- -D warnings` limpio; un test de orden de intento con dos transportes falsos | p2p, VPN nueva, relé |
| **F2** Spike iroh (medir) | Un PC y un móvil **fuera de la LAN y sin VPN**, datos móviles. Relé público **solo para medir**, nunca en producción | ≥ 20 intentos en 2 redes distintas. Registrar: % conexiones directas, % relevadas, tiempo hasta primera respuesta, fallos. Decisión: si ≥ 80 % directas y el resto con relé funciona [I, umbral a confirmar] | Relé propio, despliegue |
| **F3** VPN opcional | Documentar Tailscale como ruta opcional; el host escucha en la IP de la VPN con `--tls-bind`. Ningún código requiere la VPN | Un móvil con VPN y sin LAN llega al host por la IP de la VPN; sin VPN, el mensaje es `NoRoute` | Auto-aprobación nueva |
| **F4** Relé en producción | Solo si el Master aprueba un coste o un host alcanzable | Decisión escrita con coste medido | — |

**Resultado F2 (2026-11-01, 1 red):** iroh 1.4.1, relé público n0 solo para medir. Móvil TECNO con LTE (CGNAT, sin Wi‑Fi ni VPN) → PC en LAN doméstica: 20 rondas, **20 directas (100 %)**, 0 relé, 0 fallos. `t_primera_respuesta` mediana 1,36 s, p90 1,86 s; RTT mediano ≈ 230 ms [V]. Pendiente: una 2.ª red distinta (el criterio pide 2), desglosar los 1,4 s (bind + relé + DNS), y el panic `ndk-context … android context was not initialized` en cada ronda (el cliente es un ejecutable nativo; en la app Flutter habría que inicializar el contexto Android) [V]. RTT alto para una ruta «directa» en LTE: comprobar que no sea relé encubierto [NV].

**Resultado F2, 2.ª red (2026-11-01):** móvil en otro Wi‑Fi (192.168.1.x, datos móviles también activos) → PC en 192.168.50.x: 20/20 directas, 0 relé, 0 fallos. Tiempo hasta primera respuesta mediana 1,0 s; hasta ruta directa 1,4–2,7 s (mediana ≈1,6 s). RTT con la ruta ya directa ≈ 65 ms (mediana); los primeros pings, antes de perforar el NAT, ≈ 58 ms en mediana pero hasta 205 ms en la primera ronda, así que los ≈230 ms de la 1.ª red eran el tramo por relé [V]. Con 2 redes distintas y 40/40 directas se cumple el umbral ≥ 80 % de F2 [V]; limitación: ambas con un único NAT doméstico del lado del PC [NV].

**Qué medir antes de decidir iroh:** la tasa de conexión directa fuera de LAN (F2); el tiempo de handshake con relé; el consumo de batería del móvil en espera; y si la app Android/Flutter puede usar el crate (binding o FFI) [NV].

## 6. Riesgos

- **NAT simétrico:** la conexión directa puede fallar con frecuencia y dejar todo el tráfico en el relé. Proporción desconocida [NV] → lo mide F2.
- **Relés públicos:** sin SLA, límites no publicados y uso «desarrollo y hobby» [V]. Un relé de n0 caído bloquea P2P sin aviso.
- **Privacidad de metadatos:** el relé ve IP y horario; el servicio de descubrimiento de n0 puede relacionar EndpointId con IP [I]. El contenido sigue cifrado [V].
- **Madurez y versión:** iroh 1.x lleva meses, con cambios de API en versiones menores 0.9x–1.x [NV]. Fijar versión exacta.
- **Gap de bindings:** sin binding Kotlin/Flutter verificado, el p2p en el móvil puede exigir FFI propio [NV].
- **Headscale:** el control plane debe ser alcanzable para que el móvil reciba su mapa. Sin host público, no sirve fuera de la LAN [I].
- **Tailscale como única ruta:** ya falló una vez (APK8D, ~6 h). Por eso es opcional (R4 y §4).
- **`HostDown` no se distingue por TCP con la regla de cortafuegos actual:** con la regla de entrada por programa, un host parado hace que Windows descarte los SYN en silencio. Medido con `nc` desde el TECNO (2026-11-02): 28/28 `Timeout` de 3,2 s, ningún «Connection refused», y el `ping` sí respondía [V]. Por eso «PC apagado» y «servicio parado» se ven igual (`NoRoute`). El cliente ya trata un puerto rechazado como `HostDown` (`PinnedFailureKind.refused`), así que bastaría una regla de entrada **por puerto** (requiere admin una vez en el PC) para que el PC responda RST con el host parado [I].
- **Cambio de identidad del host:** el pin se rota con `host tls prepare|activate` [`pairing.md:210`]. Un transporte nuevo con otra identidad debe reutilizar R3 y no abrir un segundo canal de confianza [I].

## 7. Decisiones para el Master

1. **Trait `Transport` en `crates/transport`.** Recomendación: sí. Se implementa solo tras aprobar F0 y F1.
2. **Spike iroh (F2) sin compromiso.** Recomendación: sí. Medición con relé público, sin usarlo en producción.
3. **Relé en producción.** Recomendación: no por ahora. Sin relé de terceros, fuera de la LAN se requiere VPN opcional. No gastar presupuesto. Revisar tras F2.
4. **Tailscale como opción documentada, nunca requerida.** Recomendación: sí.
5. **Túnel cloudflared/ngrok.** Recomendación: descartar salvo emergencia; el borde termina TLS [I].
6. **Auto-aprobación por IP de tailnet.** Recomendación: mantener mientras exista VPN; no contar como identidad (§1.3).
7. **NetBird.** Recomendación: descartar hasta verificar su LICENSE [NV].
8. **F0 antes que cualquier transporte.** Recomendación: sí; es un cambio pequeño y arregla un fallo visible hoy.

## 8. Fuentes

- iroh, README: https://github.com/n0-computer/iroh/blob/main/iroh/README.md
- iroh, `Cargo.toml` (versión 1.4.1, `MIT OR Apache-2.0`): https://raw.githubusercontent.com/n0-computer/iroh/main/iroh/Cargo.toml
- iroh, límites de relés: https://docs.iroh.computer/relays/rate-limiting.md
- iroh, relés compartidos (precio $19/mes según búsqueda, no leído en la página): https://docs.iroh.computer/iroh-services/relays/shared.md [NV]
- iroh, conexión directa vs relé (`Connection::paths`): https://github.com/n0-computer/iroh/blob/main/iroh/src/endpoint/connection.rs
- iroh 1.0 (blog de terceros, cifras [NV]): https://pinggy.io/blog/iroh_1_0_dial_keys_not_ips/
- GitHub API, licencias: `n0-computer/iroh` Apache-2.0; `cloudflare/cloudflared` Apache-2.0; `tailscale/tailscale` BSD-3-Clause; `juanfont/headscale` BSD-3-Clause; `netbirdio/netbird` NOASSERTION (consultado 2026-10-27).
- Headscale como servidor de control, BSD-3 (búsqueda; resumen WZ-IT): https://wz-it.com/en/knowledge/remote-access/what-is-headscale/
- Context7 `/n0-computer/iroh` (relés y conexión directa).
- Repo: `docs/contracts/pairing.md`, `api.md`, `auth.md`, `errors.md`; `docs/troubleshooting-tailscale.md`; `docs/roadmap.md`; `docs/rfc/RFC-push.md` (formato); los ficheros de código de §1.1–§1.4.
