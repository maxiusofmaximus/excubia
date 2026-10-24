# Notificaciones y aprobaciones

Un agente a veces pide permiso para hacer algo: ejecutar un comando, tocar un archivo o
delegar en otro agente. Esa petición aparece en la app, en **Pendientes de ti**.

## Qué ves en cada petición

- El agente que la pide, por ejemplo *Bob necesita permiso*. Si el agente no tiene nombre,
  aparece como *Un agente*.
- El proyecto al que pertenece y el nivel de riesgo: *riesgo bajo*, *riesgo medio* o
  *riesgo alto*.
- Quién la tiene: *lo decide el lead*, *lo decide Alfred* o *te lo pasa Alfred*.
- Una cuenta atrás, si el host fija un plazo. El plazo lo marca el host, no el móvil.

Puedes responder con **Aprobar**, **Permitir una vez** o **Denegar**. Si el plazo expira, la
petición caduca y no se aprueba.

## Quién decide

Cada petición sube por niveles: la política del proyecto, el lead, Alfred y tú. Cada nivel
decide una vez. Las aprobaciones solo suben, nunca bajan. Por eso una petición puede llegar
a ti después de que el lead o Alfred la hayan visto.

El ajuste de aprobación puede ser global o de cada proyecto:

- **Activado**: cada permiso llega a ti.
- **Desactivado**: el lead y Alfred deciden antes, y a ti solo llegan los permisos que escalen.

## Si no puedes aprobar

Para aprobar desde un dispositivo hace falta el permiso `approve`. Si no lo tiene, la app te
dice que no puede cambiar la aprobación y muestra el comando para concederlo en el PC:

```powershell
host devices grant <id> --permission approve
```

## Avisos

- **Móvil.** Mientras la app está abierta, recibe los cambios en tiempo real. Con la app
  cerrada no hay avisos: las notificaciones push todavía no están implementadas.
- **Escritorio.** Muestra avisos cuando cambia el estado de un agente.

En el código, a estas peticiones se las llama «UAC». No son la ventana de control de cuentas
de Windows.
