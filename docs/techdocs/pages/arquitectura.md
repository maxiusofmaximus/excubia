# Arquitectura

herdr notify tiene tres componentes que se compilan por separado. Cada uno tiene su propio
README y su propio sistema de compilación.

| Componente | Carpeta | Qué es |
|---|---|---|
| Host | `core/` | Proceso en Rust que gestiona las sesiones de los agentes, la API cliente (HTTPS y SSE), el emparejamiento de dispositivos y las aprobaciones. |
| App Android | `apk/` | Interfaz en Flutter (`apk/mobile`) con un núcleo cliente en Rust (crate `apk_core`). Paquete `dev.herdrnotify.app`. |
| Escritorio | `desktop/` | Aplicación Tauri 2 que aloja el núcleo del host como proceso auxiliar (sidecar). |

## Crates del núcleo

- `api`: rutas HTTP del host.
- `auth`: almacén de confianza de dispositivos, permisos y soporte de Tailscale.
- `chat`: conversaciones con Alfred, delegaciones y aprobaciones.
- `detect`: detección de qué agente corre y de en qué estado está (`idle`, `working`,
  `blocked`, `unknown`).
- `host`: los binarios `host` y `hn-mcp`.
- `persist`: persistencia del estado.
- `pin`: identidad TLS del host y la regla de huella con la que los dispositivos la verifican.
- `pty`: sesiones de terminal propias del host.
- `seat`: puerto hacia los CLI de agente. La API solo conoce este puerto, nunca un CLI concreto.
- `state`: estado reducido de los agentes y ranking de atención.

## Cómo se hablan las piezas

1. El móvil y el escritorio emparejan con el host mediante un enlace o un código QR. El enlace
   lleva la huella del certificado del host (`h=`).
2. En la red local, la conexión es TLS 1.3 y el cliente solo acepta un certificado cuya huella
   coincida con la del enlace. Sobre Tailscale, la red privada cifra el tráfico. En ambos casos,
   el dispositivo se autentica con su clave, no con el transporte.
3. Los agentes avisan de su estado al host mediante hooks, con el token que imprime
   `host hook-token`.
4. El host decide qué puede hacer cada dispositivo según sus permisos. Las peticiones de
   aprobación suben por niveles: política del proyecto, lead, Alfred y, por último, el usuario.

## Piezas heredadas

El gateway de Python (`gateway/`) está obsoleto y no recibe funciones nuevas. Sus dispositivos
se importan con `host devices import-gateway`. La tabla de equivalencias está en
[paridad gateway → core](PARITY-GATEWAY.md).
