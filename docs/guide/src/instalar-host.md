# Instalar el host y el escritorio

El **host** es el proceso que corre en tu PC. El **escritorio** es la app de Windows que
lo arranca. Al instalar el escritorio ya tienes el host: no hay que instalarlo aparte.

## Instalar

Abre PowerShell y pega esta línea:

```powershell
irm https://raw.githubusercontent.com/maxiusofmaximus/excubia/main/desktop/scripts/install.ps1 | iex
```

El script descarga el instalador de la última versión de escritorio publicada, comprueba su
huella SHA-256 con la que registra GitHub y lo instala para tu usuario. Después abre
**Excubia** desde el menú Inicio.

Si prefieres hacerlo a mano, descarga el archivo `-setup.exe` de la última versión
`desktop-v…` en las [Releases de GitHub](https://github.com/maxiusofmaximus/excubia/releases)
y ejecútalo.

## Compilar desde el código

Solo hace falta si quieres modificar Excubia.

Desde la carpeta `desktop` del repositorio:

1. Compila el host y la herramienta que usa Alfred:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-host.ps1
   ```

   El script espera que el código del núcleo esté en una carpeta llamada `core` junto a
   `desktop`. Si está en otro sitio, pasa la ruta con `-CoreDir`. Copia el host y `excubia-mcp`
   a `src-tauri\binaries`. Sin la herramienta `excubia-mcp`, cada mensaje a Alfred falla con
   `409 seat_unavailable`.

2. Compila la app de escritorio. Para desarrollo:

   ```powershell
   cargo build -p excubia-desktop
   ```

   Para generar la app completa, el repositorio indica
   `pnpm dlx @tauri-apps/cli@latest build`. Necesitas las herramientas de Tauri instaladas.

En esta guía, `host` significa el ejecutable `excubia-host.exe`.

## Dónde guarda sus datos

| Qué | Dónde |
|---|---|
| Datos del escritorio y del host que arranca | `%APPDATA%\dev.excubia.desktop\core` |
| Registro del host (`host.log`) | `%LOCALAPPDATA%\dev.excubia.desktop\logs\host.log` |
| Host ejecutado sin el escritorio | `%APPDATA%\excubia` |

La carpeta del host sin escritorio se puede cambiar con la variable de entorno
`EXCUBIA_DATA_DIR` o con la opción `--data-dir`. La clave privada del escritorio se
guarda en el almacén de credenciales de Windows, no en esas carpetas.

Si vienes de una versión anterior al cambio de nombre (cuando el producto se llamaba
«herdr notify»), el host y el escritorio trasladan solos sus carpetas antiguas
(`%APPDATA%\herdr-notify`, `dev.herdrnotify.desktop`) a las nuevas la primera vez que arrancan.

## Comandos del host

El host acepta estos comandos desde una terminal. Todos admiten `--data-dir DIR`.

| Comando | Para qué sirve |
|---|---|
| `host` | Arranca el host. Escucha en `127.0.0.1` por defecto (puerto `8791`). |
| `host pair-code [--ttl S] [--lan IP:PUERTO]` | Crea un código de emparejamiento de un solo uso y el enlace para el móvil. Caduca a los 600 segundos por defecto. |
| `host tls prepare` / `host tls activate` | Prepara y activa el certificado de la red local. |
| `host devices list` | Lista los dispositivos emparejados. |
| `host devices grant <id> --permission <permiso>` | Concede un permiso a un dispositivo. |
| `host devices revoke <id>` | Quita un dispositivo. |
| `host hook-token` | Imprime el token que los agentes usan para avisar de su estado. |

Al arrancar, el host también acepta `--uac-plugin-dir DIR`, la carpeta del plugin que
bloquea a los agentes mientras esperan una aprobación. Sin esa opción, el host avisa en su
registro y las peticiones no bloquean al agente. Ver [solución de problemas](solucion-problemas.md).

La escucha en la red local está apagada por defecto. Para activarla, usa el interruptor de
red local del escritorio. Cómo se usa para emparejar está en [Emparejar el móvil](emparejar.md).

## Antes de seguir

Comprueba que el host arranca y que la carpeta de datos aparece. Si no arranca, mira el
`host.log` (ruta en la tabla de arriba) y el capítulo de [solución de problemas](solucion-problemas.md).
