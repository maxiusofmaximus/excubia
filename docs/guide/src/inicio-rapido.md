# Inicio rápido

Esta página resume el camino completo. Cada paso tiene su capítulo con más detalle.

1. **Instala la app en el móvil.** Descarga el APK de la última versión desde las
   [Releases de GitHub](https://github.com/maxiusofmaximus/excubia/releases) e instálalo.
   Detalles en [instalar la app Android](instalar-apk.md).

2. **Instala el escritorio en el PC.** Abre PowerShell y pega:

   ```powershell
   irm https://raw.githubusercontent.com/maxiusofmaximus/excubia/main/desktop/scripts/install.ps1 | iex
   ```

   Después abre Excubia desde el menú Inicio: el escritorio arranca el host. Detalles en
   [instalar el host y el escritorio](instalar-host.md).

3. **Empareja el móvil.** En el escritorio, activa la red local y pulsa **Mostrar código QR**.
   En la app, pulsa **Escanear QR**. Si no puedes usar la cámara, crea un enlace con
   `host pair-code --lan <IP-del-PC>:8443` y pégalo con **Pegar enlace**. Detalles en
   [emparejar el móvil](emparejar.md).

4. **Concede permisos.** Un móvil recién emparejado queda pendiente y no ve nada. En el PC:

   ```powershell
   host devices list
   host devices grant <id> --permission read_state
   ```

   Para aprobar peticiones desde el móvil, concede también `approve`. Los permisos están en
   [seguridad](seguridad.md).

Cuando la app muestra tus agentes, el camino está completo. Si algo falla, busca el mensaje en
[solución de problemas](solucion-problemas.md).
