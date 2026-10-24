# Seguridad en lenguaje llano

herdr notify da acceso al estado de tus agentes y a sus permisos. Estas son las reglas que
conviene conocer.

## El certificado del host tiene una huella

Cuando emparejas, el enlace trae la huella (SHA-256) del certificado del host. La app solo se
conecta si el certificado presentado coincide con esa huella. Si el host cambia de
certificado, la app te lo dice y tendrás que emparejar de nuevo.

Si la app dice *La huella no coincide: no te conectes a esta red*, no sigas. Puede ser otro
PC o una red que no es la tuya.

## Cada móvil tiene su propia identidad

Cada dispositivo tiene una clave de identidad propia. La parte privada no sale del
dispositivo ni se envía al host.

## Los dispositivos nuevos esperan

Un dispositivo recién emparejado queda pendiente y sin permisos. Tú decides qué puede hacer:

| Permiso | Qué permite |
|---|---|
| `read_state` | Ver el estado de los agentes y el chat. |
| `respond` | Responder a agentes y escribir a Alfred. |
| `send_key` | Enviar teclas a un agente. |
| `approve` | Aprobar o denegar peticiones. |
| `admin` | Gestionar otros dispositivos. |

Concede solo lo que necesite cada dispositivo:

```powershell
host devices grant <id> --permission read_state
```

## Quitar un dispositivo

```powershell
host devices list
host devices revoke <id>
```

Revocar marca el dispositivo como revocado; deja de tener acceso de inmediato.

## Si pierdes el móvil

1. En el PC, ejecuta `host devices list` y localiza el móvil.
2. Revócalo con `host devices revoke <id>`.
3. Si sospechas que alguien vio un enlace de emparejamiento, crea otro para comprobarlo. Un
   enlace ya usado no sirve, y el enlace caduca en 600 segundos.

## La red local está apagada por defecto

El host escucha solo en tu equipo hasta que activas la red local. La escucha de red local
rechaza direcciones comodín como `0.0.0.0` o `::`, así que no se abre a todas las redes por
accidente; lo comprueban las pruebas del código del host. No publiques el puerto `8443` en
Internet.
