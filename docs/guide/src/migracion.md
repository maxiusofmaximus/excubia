# Migrar desde la app anterior

La app anterior (Kotlin, versión 0.1.x) usaba el mismo identificador de aplicación y la
misma firma que la nueva, así que la actualización se instala encima. Hoy la app nueva
**no reutiliza** el emparejamiento antiguo: hay que emparejar de nuevo. La reutilización
automática está en desarrollo y todavía no se ha verificado en un dispositivo real.

## Qué hacer

1. Actualiza a la app nueva, como en [instalar la app](instalar-apk.md).
2. Empareja de nuevo con un código del host, como en [emparejar el móvil](emparejar.md).
3. Cuando el emparejamiento funcione, revisa que el móvil ve el estado de tus agentes. Si
   está pendiente, concédele `read_state` en el host.

