# Migrar desde la app anterior

La app anterior (Kotlin, versión 0.1.x) usaba el mismo identificador de aplicación y la
misma firma que la nueva, así que la actualización se instala encima. Hoy la app nueva
**no reutiliza** el emparejamiento antiguo: hay que emparejar de nuevo. La reutilización
automática está en desarrollo (ver `apk/docs/migration-from-kotlin.md`).

## Qué hacer

1. Actualiza a la app nueva, como en [instalar la app](instalar-apk.md).
2. Empareja de nuevo con un código del host, como en
   [emparejar el móvil](emparejar.md).
3. Cuando el emparejamiento funcione, revisa que el móvil ve el estado de tus agentes.
   Si está pendiente, concédele `read_state` en el host.

## Si usabas el gateway de Python

El gateway de Python está obsoleto. Quien tenga dispositivos emparejados con él puede
importarlos en el host actual con `host devices import-gateway <devices.json>`: los
dispositivos conservan su identificador y sus permisos, y se vuelven a unir con su código
de recuperación.
