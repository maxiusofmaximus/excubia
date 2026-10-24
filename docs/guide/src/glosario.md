# Glosario

**Agente.** Programa de programación, como Claude Code u opencode, que corre en tu PC y que el
host observa. Los agentes avisan de su estado al host mediante hooks.

**Alfred.** El orquestador: una conversación persistente con el host desde la que se lanzan
agentes y se delegan tareas.

**Aprobación.** Decisión sobre una petición de permiso de un agente. Puede ser **Aprobar**,
**Permitir una vez** o **Denegar**.

**Código de emparejamiento.** Código de un solo uso que el host crea con `host pair-code`. Va
dentro del enlace `herdr-notify://pair?…` o del código QR, y caduca a los 600 segundos por
defecto.

**Código de recuperación.** Código que permite volver a unir un dispositivo al host sin crear una
identidad nueva. Se usa al importar desde el gateway Python o al migrar desde la app anterior.

**Dispositivo.** Un móvil o un escritorio emparejado con el host. Su identidad es una clave
pública; la parte privada no sale del dispositivo.

**Escritorio.** La app de Windows (Tauri) que arranca el host como proceso auxiliar y muestra el
chat y el equipo de agentes.

**Especialista.** Agente al que Alfred delega una parte del trabajo. Va después del lead del
proyecto en la cadena de delegación.

**Estados de un agente.** `blocked` (espera tu decisión), `done` (ha terminado y no lo has visto),
`working` (trabajando) e `idle` (inactivo). Por ese orden se ordenan los agentes que requieren
atención.

**Gateway (Python).** La versión anterior del host, escrita en Python. Está obsoleta.

**Host.** El proceso que corre en tu PC. Es el único que habla con los agentes, guarda su estado y
decide qué puede hacer cada dispositivo.

**Huella (pin).** Resumen SHA-256 de la clave pública del certificado del host. El enlace de
emparejamiento la lleva en `h=`, y el móvil solo se conecta si el certificado coincide.

**Lead.** Agente que coordina un proyecto dentro de la cadena de delegación de Alfred. Puede
decidir sobre algunas peticiones antes que tú.

**Permiso.** Capacidad concedida a un dispositivo: `read_state`, `respond`, `send_key`, `approve`
o `admin`. Ver [seguridad](seguridad.md).

**Pendiente.** Estado de un dispositivo recién emparejado: el host lo conoce, pero no le ha
concedido permisos.

**Red local (LAN).** Escucha cifrada del host en la IP privada de tu PC, en el puerto `8443`. Está
apagada por defecto.

**Tailscale.** Red privada virtual opcional que permite conectar el móvil y el PC fuera de tu red
local.

**TLS 1.3.** Versión del protocolo de cifrado de la conexión entre la app y el host.

**UAC.** Nombre interno de las peticiones de aprobación de los agentes. No es la ventana de control
de cuentas de Windows.
