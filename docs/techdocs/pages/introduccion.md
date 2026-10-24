# Documentación técnica

Esta sección describe cómo funciona herdr notify por dentro: los contratos entre componentes,
las propuestas de diseño (RFC), la revisión de seguridad y la paridad con el gateway de Python.

Los contratos y las RFC están en inglés. La portada, la arquitectura y la navegación están en
español.

## Dónde está cada cosa

| Documento | Para qué sirve |
|---|---|
| [Arquitectura](arquitectura.md) | Visión general del host, la app Android y el escritorio, y cómo se hablan. |
| [Contratos](contracts/) | Formas exactas de peticiones, respuestas, estado y emparejamiento. |
| [RFC](rfc/RFC-transports.md) | Propuestas de diseño, con sus decisiones abiertas. |
| [Revisión de seguridad](security-review.md) | Qué se ha revisado y qué queda pendiente. |
| [Paridad gateway → core](PARITY-GATEWAY.md) | Qué hace el gateway de Python y qué equivalente tiene el host. |
| [Migración desde la app Kotlin](apk/migration-from-kotlin.md) | Cómo reutilizar el emparejamiento de la app anterior. |

## Estado de la implementación

Los contratos marcan cada concepto con una etiqueta: `IMPLEMENTED`, `NOT IMPLEMENTED`,
`DEFERRED` o `RESERVED`. Una forma definida no significa que el camino esté operativo. Manda
la etiqueta.

## De dónde sale esta sección

Los documentos se copian desde su sitio original al construir el libro, con
`docs/techdocs/build.sh`. No hay copias que mantener: para cambiar un contrato, edita su
fichero en `core/docs/`.
