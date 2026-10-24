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
- Los dos en la misma red local, o en la misma red de Tailscale. Tailscale es opcional.

## Lo que todavía no está disponible

- **Avisos con la app cerrada.** Las notificaciones push (FCM) todavía no están
  implementadas. Mientras la app está abierta, los cambios llegan en tiempo real.
- **Instalador del escritorio.** Todavía no hay un instalador publicado. Hoy el host y el
  escritorio se compilan desde el código.

## Cómo leer esta guía

Si es tu primera vez, sigue el [inicio rápido](inicio-rapido.md). Los capítulos de
[seguridad](seguridad.md) y [solución de problemas](solucion-problemas.md) sirven cuando algo
no encaja. El [glosario](glosario.md) explica los términos que aparecen en la app.
