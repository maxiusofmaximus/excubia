# Emparejar el móvil

Emparejar crea una relación de confianza entre el móvil y el host. Se hace una vez por
móvil. Antes, el host tiene que estar en marcha en tu PC.

El enlace de emparejamiento tiene tres datos importantes:

- **El código**: sirve una sola vez.
- **La huella** (`h=`): identifica el certificado del host. La app solo se conecta si el
  certificado coincide.
- **La caducidad** (`e=`): por defecto, 600 segundos desde que se crea.

## Opción 1: escanear el código QR

1. En el escritorio, activa la red local y pulsa **Mostrar código QR**.
2. En la app, pulsa **Escanear QR** y apunta la cámara a la pantalla del PC.
3. La app se empareja. Si la conexión está cifrada, muestra
   *Conexión cifrada (TLS 1.3), verificada con la huella del código.*

## Opción 2: pegar el enlace

1. En el PC, crea el enlace con la IP privada del equipo y el puerto `8443`. Sustituye
   `<IP-del-PC>` por la de tu equipo:

   ```powershell
   host pair-code --lan <IP-del-PC>:8443
   ```

2. Copia el enlace que imprime. Empieza por `herdr-notify://pair?`.
3. En la app, pulsa **Pegar enlace** y pega el enlace.

Si la app no llega al PC, añade la dirección en **Avanzado** o usa el QR. Si cambia la IP
del PC, crea un enlace nuevo.

## Después de emparejar

El móvil queda **pendiente**: el host lo conoce, pero no le concede ningún permiso. Hasta
que no lo autorices, la app no mostrará el estado de tus agentes. En el PC:

```powershell
host devices list
host devices grant <id> --permission read_state
```

Los permisos y su significado están en [Seguridad](seguridad.md).

## Si el enlace falla

Un enlace caducado o usado ya no sirve. Crea otro con `host pair-code`. Si la app dice que la
huella no coincide, no sigas: es otro host o otra red. Más casos en
[solución de problemas](solucion-problemas.md).
