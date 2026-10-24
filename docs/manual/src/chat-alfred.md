# Chat con Alfred

Alfred es el orquestador: una conversación persistente con el host. Desde ahí puedes
pedirle que lance un agente, que delegue trabajo y que te resuma lo que ha pasado. Las
delegaciones pasan por niveles: Alfred, el lead del proyecto y los especialistas.

## Las dos pestañas

- **Chat**: la conversación con Alfred, con su historial.
- **Equipo**: la lista de agentes, el detalle de cada uno y las acciones para responder,
  parar un agente o abrir una conversación nueva. En el escritorio, **Equipo** está a un
  toque en el panel lateral.

## Escribir a Alfred

1. Escribe en **Mensaje a Alfred** y envía.
2. El mensaje aparece al enviarlo. Cuando el host lo acepta, pasa a estado *recibido*.
3. Si no hay conexión, verás *Reconecta para escribir a Alfred*. Hasta que vuelvas a
   conectar, no podrás enviar.

## Conversaciones

- **Nuevo chat con Alfred** abre otra conversación.
- Una conversación archivada se puede leer, pero para escribir en ella hay que
  desarchivarla antes.

## Parar un agente

Desde **Equipo** puedes parar un agente en marcha. Si el agente ya no existe, el host lo
indica y no hace nada más.

## Mensajes de error habituales

- *Alfred no pudo arrancar*: el host no pudo iniciar su proceso. Mira el registro del host
  en [instalar el host](instalar-host.md).
- *La cola de Alfred está llena*: hay demasiadas peticiones pendientes. Espera un momento
  y vuelve a enviar.
- *La delegación supera la profundidad permitida*: la cadena Alfred → lead → especialista
  tiene un límite. Pide la tarea de forma más directa.
