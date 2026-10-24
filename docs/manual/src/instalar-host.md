# Instalar el host y el escritorio

El **host** es el proceso que corre en tu PC. El **escritorio** es la app de Windows que
lo arranca. Todavía no hay un instalador publicado para el escritorio, así que hoy se compila desde
el código.

## Compilar

Desde la carpeta `desktop` del repositorio:

1. Compila el host y la herramienta que usa Alfred:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-host.ps1
   ```

   El script espera que el código del núcleo esté en una carpeta llamada `core` junto a
   `desktop`. Si está en otro sitio, pasa la ruta con `-CoreDir`. Sin la herramienta
   `hn-mcp`, cada mensaje a Alfred falla con `409 seat_unavailable`.

2. Compila la app de escritorio. Para desarrollo:

   ```powershell
   cargo build -p herdr-notify-desktop
   ```

   Para generar la app completa, el repositorio indica `pnpm dlx @tauri-apps/cli@latest build`. Necesitas las herramientas de Tauri instaladas.

## Dónde guarda sus datos

| Qué | Dónde |
|---|---|
| Datos del escritorio y del host que arranca | `%APPDATA%\dev.herdrnotify.desktop\core` |
| Registro del host (`host.log`) | `%LOCALAPPDATA%\dev.herdrnotify.desktop\logs\host.log` |
| Host ejecutado sin el escritorio | `%APPDATA%\herdr-notify` |

La carpeta del host sin escritorio se puede cambiar con la variable de entorno
`HERDR_NOTIFY_DATA_DIR` o con la opción `--data-dir`. La clave privada del escritorio
se guarda en el almacén de credenciales de Windows, no en esas carpetas.

## Comandos del host

El host acepta estos comandos desde una terminal. Todos admiten `--data-dir DIR`.

| Comando | Para qué sirve |
|---|---|
| `host` | Arranca el host. Escucha en `127.0.0.1:8791` por defecto. |
| `host pair-code [--ttl S] [--lan IP:PUERTO]` | Crea un código de emparejamiento de un solo uso y el enlace para el móvil. Caduca a los 600 segundos por defecto. |
| `host tls prepare` / `host tls activate` | Prepara y activa el certificado de la red local. |
| `host devices list` | Lista los dispositivos emparejados. |
| `host devices grant <id> --permission <permiso>` | Concede un permiso a un dispositivo. |
| `host devices revoke <id>` | Quita un dispositivo. |
| `host hook-token` | Imprime el token que los agentes usan para avisar de su estado. |

La escucha en la red local está apagada por defecto. Para activarla, usa el interruptor
de red local del escritorio. Cómo se usa para emparejar está en [Emparejar el móvil](emparejar.md).

## Antes de seguir

Comprueba que el host arranca y que la carpeta de datos aparece. Si no arranca, mira el
`host.log` (ruta en la tabla de arriba) y el capítulo de [solución de problemas](solucion-problemas.md).
