# Solución de problemas

Busca el mensaje que ves en la tabla. Si no aparece, mira el registro del host en
`%LOCALAPPDATA%\dev.herdrnotify.desktop\logs\host.log`.

| Lo que ves | Qué significa | Qué hacer |
|---|---|---|
| *Este teléfono ya no está emparejado* | El host ya no reconoce el dispositivo. Se revocó o cambió de datos. | Empareja de nuevo con un código nuevo. |
| *El certificado del equipo ya no es válido. Vuelve a emparejar desde el equipo.* | El host cambió su certificado. | Crea un código nuevo con `host pair-code` y empareja. |
| *La huella no coincide: no te conectes a esta red* | El certificado no es el del host que emparejaste. | No sigas. Revisa que estás en tu red y en tu PC. |
| *El enlace de emparejamiento no es válido* | El enlace está mal copiado, caducó o ya se usó. | Crea un enlace nuevo. |
| *Este código QR no es de herdr notify* | Escaneaste otro código. | Escanea el QR del escritorio. |
| *Este código QR es de una versión más nueva; actualiza la app.* | El host usa un formato que la app no conoce. | Actualiza la app. |
| *Reconecta para escribir a Alfred* | Se cortó la conexión con el host. | Espera o reconecta. |
| *Alfred no pudo arrancar* | El host no pudo iniciar a Alfred. | Mira `host.log`. |
| El móvil está emparejado pero no ve nada | El dispositivo está pendiente y sin permisos. | Concede `read_state` con `host devices grant`. |
| Los agentes no piden aprobación | El host arrancó sin el plugin de aprobación. | Ejecuta el host con `--uac-plugin-dir` apuntando a la carpeta del plugin **[NV]**. |

## El host no arranca por un 401 de Tailscale

Si el mensaje es parecido a *Tailscale already in use* con un error 401, otra sesión de
Windows tiene bloqueado el estado de Tailscale. Es habitual cuando hay una sesión de
escritorio remoto desconectada. Los pasos, desde una PowerShell **como administrador**:

1. Mira las sesiones abiertas con `query user`.
2. Cierra los procesos `tailscale-ipn` que pertenezcan a otra sesión:

   ```powershell
   Get-Process tailscale-ipn |
     Where-Object { $_.SessionId -ne (Get-Process -Id $PID).SessionId } |
     Stop-Process -Force
   ```

3. Reinicia el servicio y comprueba el estado:

   ```powershell
   Restart-Service Tailscale
   tailscale status
   ```

Si el 401 sigue, cierra la sesión de escritorio remoto que esté desconectada. Al cerrarla,
el bloqueo se libera.

Ten en cuenta que si el host no puede consultar la dirección de Tailscale al arrancar,
solo escucha en loopback. El teléfono deja de llegar aunque el proceso esté vivo.
