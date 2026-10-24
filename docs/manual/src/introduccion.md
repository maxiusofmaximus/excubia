# Qué es herdr notify

herdr notify te permite ver desde el móvil lo que hacen tus agentes de programación
que corren en tu PC, y responder a sus peticiones sin estar delante. Tiene tres piezas:

- **La app Android** (`dev.herdrnotify.app`). Muestra tus agentes, las peticiones que
  esperan tu decisión y el chat con Alfred. Puedes aprobar o denegar desde ella.
- **El host**. Es un proceso que corre en tu PC. Es el único que habla con los agentes,
  guarda su estado y decide qué puede hacer cada dispositivo.
- **El escritorio**. Es la app de Windows (Tauri) con el mismo chat y el equipo de
  agentes. Arranca el host como proceso auxiliar.

El móvil nunca habla directamente con los agentes. Siempre pasa por el host, y solo
después de [emparejarse](emparejar.md) con él.

## Qué necesitas

- Un PC con Windows donde corre el host (o el escritorio).
- Un móvil Android.
- Los dos en la misma red local, o en la misma red de Tailscale.

## Cómo leer esta guía

Empieza por [instalar la app](instalar-apk.md) y [emparejar el móvil](emparejar.md).
Los capítulos de [seguridad](seguridad.md) y [solución de problemas](solucion-problemas.md)
sirven cuando algo no encaja. Donde algo todavía no está disponible, la guía lo dice claramente.
