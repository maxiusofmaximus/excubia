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
| *La cola de Alfred está llena* | Hay demasiadas peticiones pendientes. | Espera un momento y vuelve a enviar. |
| *409 `seat_unavailable`* | El host no encuentra `hn-mcp`, la herramienta que usa Alfred. | Compila el host con `build-host.ps1`, que también compila `hn-mcp`, y reinicia el host. |
| El móvil está emparejado pero no ve nada | El dispositivo está pendiente y sin permisos. | Concede `read_state` con `host devices grant`. |
| Los agentes no piden aprobación | El host arrancó sin `--uac-plugin-dir`, así que las peticiones no bloquean al agente. El registro lo indica. | Arranca el host con `--uac-plugin-dir` apuntando a la carpeta del plugin de aprobación. |

Si el mensaje no está en la tabla, busca su texto exacto en el registro del host.
