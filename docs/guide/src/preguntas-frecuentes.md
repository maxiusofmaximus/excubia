# Preguntas frecuentes

## ¿Necesito Tailscale?

No. Tailscale es opcional. Puedes usar la red local: el host escucha en tu PC y el móvil se
conecta a él por la IP privada, con la conexión cifrada y la huella del certificado.

## ¿Recibo avisos si la app está cerrada?

No todavía. Las notificaciones push (FCM) no están implementadas. Mientras la app está abierta,
los cambios de tus agentes llegan en tiempo real.

## ¿El móvil habla con algún servidor externo?

La app se conecta solo al host que emparejaste, y los agentes y su estado viven en tu PC. El
producto no tiene cuentas de usuario.

## ¿Puedo emparejar varios móviles?

Sí. Cada dispositivo se empareja por separado, tiene su propia clave y sus propios permisos.
Revocar uno no afecta a los demás.

## ¿Hace falta el escritorio para usar el host?

No. El host puede correr sin la app de escritorio. El escritorio es la forma cómoda de ver el
chat, mostrar el código QR y activar la red local.

## ¿El host tiene que estar en marcha todo el tiempo?

Sí. El móvil solo habla con el host. Si el host está parado, la app no puede ver a tus agentes
ni responderles.

## ¿Dónde están mis datos?

En tu PC. La tabla de carpetas está en [instalar el host y el escritorio](instalar-host.md).

## ¿Qué hago si cambia la IP de mi PC?

El enlace de emparejamiento lleva la IP del PC. Crea un enlace nuevo con la IP actual y empareja
de nuevo, o usa el código QR del escritorio.

## ¿Dónde reporto un problema de seguridad?

Por el canal privado de GitHub: las *security advisories* del repositorio. No abras un
issue público con los detalles de la vulnerabilidad.

## ¿Dónde está el código?

En [GitHub](https://github.com/maxiusofmaximus/herdr-notify). La documentación técnica de los
contratos y las RFC está en [documentación técnica](../docs/index.html).
