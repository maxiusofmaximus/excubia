# RFC: transportes APK / Desktop ↔ host sin depender de Tailscale

Estado: propuesta · 2026-10-23 · Origen: TRANSPORT1 · **[V]** verificado (fuente Fn de `TRANSPORT1-FUENTES.md` o ruta:línea) · **[I]** inferencia · **[NV]** no verificado

## 1. Problema
- El móvil solo alcanza el host por tailnet (100.64/10) o loopback con `adb reverse` [V: `apk/mobile/lib/core/api/pairing_address.dart:21-22`, `tailnet_cleartext.dart:16-18`].
- El host escucha solo en loopback; «llegar al teléfono es trabajo del transporte» (Tailscale) [V: `core/docs/contracts/api.md:3-4`].
- Emparejamiento actual: código de 4 bytes, 600 s, un uso; la auto-aprobación depende de `tailscale whois` [V: `core/docs/contracts/pairing.md:80-97`].
- Objetivo: escanear un QR y que funcione en casa y fuera, sin servicio externo obligatorio, con la máxima seguridad. Presupuesto cero.

## 2. Modelo de amenazas
- **Activos:** capacidad de enviar prompts y teclas a agentes (el host es «la joya de la corona») [V: `apk/PRODUCT.md:64-65`]; claves de dispositivo en el Keystore; código de emparejamiento; estado de los agentes.
- **Atacantes:** (a) red local hostil (Wi-Fi público/empresa, ARP, AP falso); (b) Internet; (c) relé o servicio de terceros, curioso o comprometido; (d) quien ve o fotografía el QR; (e) robo del móvil; (f) dependencia Rust con CVE.
- **Supuestos:** el Keystore del móvil no está comprometido; el usuario escanea el QR de su propio host; el QR se muestra en pantalla del host, no sale del dispositivo.

| Amenaza | Mitigación propuesta | Estado |
|---|---|---|
| MITM en LAN | Huella (SPKI) del host dentro del QR: ancla fuera de banda, sin CA [V: F9, F10] | [I] |
| Relé malicioso | Cifrado extremo a extremo; el relé no tiene claves de dispositivo [V: F4] | [I] |
| Replay | Cache de nonce y ventana de skew ya existentes [V: `core/docs/core/HERDR-MAP.md:75`] | existente |
| Fuga del QR | Código de un uso y 600 s; confirmación en host para dispositivos nuevos (D3) | [I] |
| Robo del móvil | Clave no exportable; revocación inmediata [V: `pairing.md:174-190`] | existente |
| Downgrade | Solo TLS 1.3 y nunca cleartext fuera de loopback [V: F11] | [I] |
| DoS en handshake | QUIC descarta paquetes no autenticados fuera de camino [V: F12, §21.2]; rate-limit por IP existente [V: `pairing.md:101-102`] | parcial |
| Host expuesto a Internet | Sin port-forward; solo salida hacia relé (ver §5) | [I] |
| Metadatos (quién, cuándo) | Ocultos para el contenido, visibles para el relé; mitigación parcial, referencia F25 | aceptado (D5) |
| CVE en dependencias | Auditoría y plan de parches | [V: F20, F21] |

## 3. Principios
- **El transporte no es la autenticación.** Cada petición sigue firmada por la clave del dispositivo; el transporte es intercambiable.
- Mínimo privilegio: pairing concede solo `READ_STATE`; escribir o aprobar lo decide el host.
- Pin de la clave del host, no confianza en CA; rotación con pin de respaldo [V: F9, F10 Appendix B].
- Escuchar solo donde hace falta: LAN en una IP explícita, nunca wildcard (regla del host, contexto de la tarea).
- Revocación inmediata y auditable (revocar marca, no borra) [V: `pairing.md:184-190`].
- Cero cuentas, cero coste: nada que exija registrarse o pagar.

## 4. Opciones evaluadas

| Opción | Seguridad | Facilidad | Dependencia externa | Coste | Madurez | Esfuerzo | NAT / CGNAT | Batería / latencia |
|---|---|---|---|---|---|---|---|---|
| **A.** LAN TLS 1.3 + pin en QR | Alta: pin fuera de banda, sin CA | Alta: QR | Ninguna | 0 | Alta: TLS y pinning estándar [V: F9, F10] | Bajo-medio (prototipo hecho) | Solo misma red | Muy baja; loopback 0,5 ms [V: prototipo] |
| **B.** iroh (QUIC, NodeId, hole punching, relés) | Alta: E2E QUIC, NodeId = clave [V: F4] | Alta: NodeId en QR | Relés públicos rate-limited o propios [V: F4] | 0 con relés públicos; VPS para propio [NV precio] | Media: 1.x en 2026; avisos en stack QUIC [V: F42-F44, secundarias] | Alto | Hole punching ~9 de 10 según FAQ [V: F6, vendor] | Relé mantiene WebSocket abierto [V: F4]; batería [NV] |
| **C.** Tailscale (estado actual) | Alta: WireGuard | Media: exige VPN activa en el móvil | SaaS de control | Plan gratis [NV] | Alta | Ya integrado | Excelente, DERP [V: F1] | Batería de VPN [I]; avisos TS-2026 en Serve/Funnel [V: F2] |
| **D.** Headscale + clientes Tailscale | Alta, sin auditoría [V: F40, snippet] | Baja-media: sigue la VPN en el móvil | Ninguna; control server público | VPS | Media: CVE en la UI Headplane [V: F46]; release 0.28.0 con CVE [V: F45] | Medio | Igual que C | Igual que C |
| **E.** NetBird (cliente y management propios) | Alta: WireGuard | Media: app VPN propia | Opcional autoalojado | 0 autoalojado | Media-alta; AGPLv3 desde v0.53.0 [V: F28] | Alto | Relé propio | VPN [I] |
| **F.** WireGuard directo (puerto abierto o VPS) | Alta criptográfica; expone un puerto UDP | Baja: config manual | VPS o router | VPS | Alta: en kernel Linux desde 5.6 [V: F17] | Medio | Falla con CGNAT [I] | Baja |
| **G.** Cloudflare Tunnel (solo salida) | Media-alta: TLS termina en Cloudflare; hace falta E2E propia [I] | Media | SaaS | Gratis, pero túnel con nombre pide cuenta y dominio [V: F22; F23 snippet] | Alta | Medio | Sin problema NAT | Latencia extra [I]; quick tunnels sin SLA [V: F23] |
| **H.** Tor onion services | Alta, anonimato | Baja: cliente Tor en móvil | Red Tor | 0 | Alta en escritorio; móvil [NV] | Alto | Sin problema NAT | Latencia y batería altas [I] |
| **I.** WebRTC data channels | Media-alta: DTLS, sin pin nativo | Baja-media: señalización propia | STUN público; TURN de pago [I] | TURN de pago [I] | Alta en navegador; plugin Flutter [NV] | Alto | Mejor del grupo [V: ICE, F13] | Media [I] |
| **J.** Noise IK a nivel de app | Alta: identidad oculta, pin de clave [V: F16 §7.8] | Alta si hay QR | Ninguna (usa A o B) | 0 | Media; implementación Rust [NV] | Medio-alto | Necesita relé o NAT | Baja [I] |
| **K.** BT / Wi-Fi Aware / NFC | — | Alta | Ninguna | 0 | — | Medio | No aplica | Solo sirve para emparejar, no como transporte |

**Lo bueno y lo malo de las principales:**
- **A:** la mejor relación seguridad/simplicidad para casa; no sirve fuera de casa; depende de que el móvil y el host estén en la misma red.
- **B:** una sola clave (NodeId) para ambas rutas, E2E y hole punching; pero la red de relés por defecto es de un tercero, es nueva y su stack QUIC tiene avisos secundarios.
- **C/D/E:** resuelven fuera de casa con poco código, pero siempre hay una VPN que activar en el móvil, que es justo el problema que se quiere eliminar.
- **G:** el host no escucha en Internet, pero el TLS termina en un tercero y la cuenta es necesaria para túneles estables.
- **I:** el mejor NAT traversal, pero exige señalización y TURN (de pago en la práctica).

## 5. Recomendación: combinación por capas con transporte intercambiable

Igual que `research/RFC-model-backends.md`: un trait `Transport` y un adaptador por tipo.

- **L1, casa u oficina (por defecto): A.** Listener TLS 1.3 en una IP LAN explícita, certificado autofirmado, huella SPKI en el QR. mDNS/DNS-SD (RFC 6762 [V: F15]) solo ayuda a descubrir; nunca es confianza.
- **L2, fuera de casa: B (iroh), solo salida.** El host marca hacia el relé y el móvil hace lo mismo; el NodeId del host va en el QR y sirve de pin para ambas rutas. Arranque con relés públicos; relé propio cuando haya presupuesto.
- **L3, opcional: C, D, E o G** para quien ya los usa. Nunca requeridos.
- **Descartados como base:** F (superficie y CGNAT), H (latencia y batería), I (señalización y TURN), K (solo pairing).
- **Reserva:** J (Noise IK) si el spike de iroh no pasa; usa el mismo transporte de A o B.

**Escuchar o solo salir.** Escuchar en Internet queda rechazado (port-forward y CGNAT). L1 escucha en IP explícita. L2 abre un socket UDP saliente; QUIC descarta lo no autenticado [V: F12 §21.2], pero esto debe probarse [NV].

## 6. UX: «escanear el QR y listo»
- QR con pocos cientos de bytes: `v` (versión), `h` (SHA-256 del SPKI del host), `n` (NodeId), `a` (IPs LAN candidatas), `r` (relés), `c` (código de un uso), `exp` (caducidad) [I].
- Flujo: el móvil escanea → intenta LAN con pin `h` → si falla, usa `n` por relé → `POST /v1/pair` con código y clave pública (contrato existente) → el host confirma (D3) → las siguientes veces reconecta solo, sin pedir IP ni VPN.
- Errores con mensaje accionable: «host no alcanzable: ¿misma red?» o «relé no disponible».

## 7. Impacto por componente (ahora solo lectura; nada cambiado)
- **Host (`core`):** listener TLS en IP LAN configurada (rechaza 0.0.0.0); identidad del host con rcgen (prototipo: ECDSA P-256); adaptador iroh solo salida; hooks siguen en loopback; contrato `POST /v1/pair` sin cambios; auto-aprobación por tailnet pasa a ser una vía opcional de C/D.
- **APK:** verificador de huella en `apk/mobile/rust` con rustls, como el prototipo, en vez del stack HTTP de Dart [I]; retirar la excepción de cleartext para IP privada y el rechazo en release (`pairing_address.dart:21-22`) [I]; la configuración por defecto de Android ya bloquea cleartext en apps con target ≥ 28 [V: F19].
- **Desktop:** mismo verificador; ya usa rustls vía reqwest [V: `cargo tree`, desktop].

## 8. Migración por fases
- **F0 (ya, sin código):** parches de `rustls-webpki` y `webpki-roots` en desktop.
- **F1:** integrar A en host y APK con tests de contrato: cert nuevo rechazado, rotación con respaldo, expiración, huella mal formada.
- **F2:** spike iroh: medir hole punching en casa, 4G, Wi-Fi empresa; batería; tamaño del binario. Criterio de salida: conexión remota en 3 redes distintas, incluida 4G/CGNAT, con batería medida y sin datos en claro en el relé.
- **F3:** B como remoto por defecto si F2 pasa; relé propio opcional.
- **F4:** adaptadores opcionales C/D/E/G.

## 9. Riesgos abiertos
- iroh es joven; la docs cambió de NodeId a EndpointId [V: F5]; su stack QUIC tiene avisos secundarios [V: F42-F44].
- Relés públicos de terceros: disponibilidad, límites y metadatos.
- Pin de certificado se rompe al renovar: usar SPKI y pin de respaldo [V: F8, F9].
- El prototipo no comprueba fechas de validez [V: código]: hay que añadirlo.
- Si se pierde la clave del host, todos los móviles deben re-emparejar [I].
- Hole punching en hard NAT: la cifra de Tailscale (≈28 min para 99,9 %) es de 2020 [V: F1].

## 10. Decisiones pendientes
- **D1.** ¿A (LAN TLS con pin en QR) como transporte primario? **Recomendado: sí.**
- **D2.** ¿Relés públicos de terceros en F2 o solo LAN hasta tener VPS? **Recomendado: relés públicos en F2; propio con presupuesto.**
- **D3.** ¿Confirmar en el host cada dispositivo nuevo aunque el QR sea válido? **Recomendado: sí.** Cierra el riesgo de un QR fotografiado.
- **D4.** ¿Pin por SPKI con respaldo en vez de pin de certificado? **Recomendado: SPKI con respaldo.**
- **D5.** ¿Aceptar que un relé vea metadatos (quién conecta y cuándo)? **Recomendado: aceptar en F2 con aviso; revisar en F3.**
- **D6.** ¿C, D, E y G solo como adaptadores opcionales? **Recomendado: sí.** G exige cuenta y dominio: no crearlos sin tu autorización (presupuesto cero).
- **D7.** ¿No empaquetar NetBird (AGPLv3) y revisar licencias de iroh, rustls y snow [NV]? **Recomendado: sí.**
- **D8.** ¿Cuándo aplicar los parches de desktop? **Recomendado: en la próxima ventana sin compilaciones concurrentes.**
