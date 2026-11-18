# Política de seguridad

## Cómo avisar de una vulnerabilidad

Usa el aviso privado de GitHub: pestaña **Security → Report a vulnerability** de este repositorio. No abras una incidencia pública ni un PR con detalles de un fallo sin corregir.

Incluye la versión (de la app o del commit), qué pasa, cómo reproducirlo y qué impacto crees que tiene. No hace falta que propongas el arreglo.

## Qué esperar

Excubia lo mantiene una sola persona, sin presupuesto ni programa de recompensas. El objetivo, sin garantía contractual:

- confirmar la recepción en pocos días;
- decir si lo consideramos una vulnerabilidad y cómo de grave, en una o dos semanas;
- corregir primero lo que permita acceso sin permiso, fuga de datos o ejecución de código, y publicar la versión corregida antes de dar detalles.

Si lo pides, te mencionamos como descubridor en el aviso publicado.

## Qué versiones se corrigen

Solo la última versión publicada del escritorio (`desktop-v*`) y de la app móvil (`v*`).

## Dentro y fuera de alcance

Dentro: el pairing de dispositivos, el transporte entre móvil y PC, la API del anfitrión, las aprobaciones de acciones de los agentes, el manejo de correo y de documentos, y los instaladores publicados.

Fuera: fallos que exigen ya tener control del PC o del móvil del usuario, ataques de denegación de servicio por fuerza bruta, ingeniería social, y vulnerabilidades en servicios de terceros (Microsoft Graph, GitHub, los proveedores de modelos).

## Qué no hemos hecho

Esto es un proyecto independiente: no tiene auditoría de seguridad externa, certificaciones (ISO 27001, SOC 2, PCI DSS) ni prueba de intrusión independiente. Quien lo evalúe para un uso empresarial o regulado debe hacerlo por su cuenta.
