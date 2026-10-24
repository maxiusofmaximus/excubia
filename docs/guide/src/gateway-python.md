# Migrar desde el gateway Python

El gateway de Python es la versión anterior del host. Está obsoleto: ya no recibe funciones
nuevas, y su trabajo lo hace el host en Rust. Si tenías dispositivos emparejados con él, puedes
traerlos al host actual sin emparejarlos desde cero.

## Importar los dispositivos

Con el fichero `devices.json` del gateway a mano, ejecuta la importación. El comando muestra un
informe por dispositivo:

```powershell
host devices import-gateway <ruta\a\devices.json>
host devices import-gateway <ruta\a\devices.json> --force
```

- Sin `--force`, las claves en conflicto no se reemplazan.
- `--force` reemplaza las claves de esos dispositivos en conflicto.

La importación no modifica el fichero del gateway, y es idempotente: repetirla no cambia el
resultado.

## Qué se conserva

- El identificador (`device_id`), los permisos, la revocación y el hash del código de
  recuperación.
- Un dispositivo importado vuelve a unirse con su código de recuperación. La app envía ese
  código en `POST /v1/pair`, y el host rota el código después.

## Qué no se importa

- El token de notificaciones (`fcm_token`). Las notificaciones push todavía no están
  implementadas.
- Los códigos de emparejamiento pendientes. Se crean de nuevo con `host pair-code`.
- Los dispositivos sin código de recuperación. Hay que emparejarlos otra vez.

## Lo que el host todavía no hace

Algunos comandos del gateway no tienen equivalente en el host actual: `revoke-permission`,
`add-device`, `dump` y `status`. Las notificaciones push tampoco están disponibles. La tabla
completa de equivalencias está en la [paridad gateway → core](../docs/PARITY-GATEWAY.html).
