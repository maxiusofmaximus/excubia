# VPN opcional (Tailscale u otra) para llegar al PC fuera de casa

Excubia no necesita ninguna VPN. Dentro de la red de casa el móvil llega al host por la LAN con TLS fijado a su clave (`docs/rfc/transport.md` §4). Una VPN solo añade una **ruta más** para cuando el móvil está fuera. Si no hay VPN ni LAN, la app dice «No llego al PC por ninguna ruta…» (`NoRoute`) y no inventa otra.

## Qué hace falta

1. **Una VPN que dé una IP fija al PC y otra al móvil** (Tailscale, WireGuard, Headscale…). Excubia no la instala ni la configura.
2. **El host escuchando en la IP de la VPN**, con una dirección concreta (los comodines `0.0.0.0` y `::` se rechazan). `--tls-bind` se repite, así que puede escuchar en la LAN y en la VPN a la vez:

   ```
   host --tls-bind <ip-lan>:8443 --tls-bind <ip-de-la-vpn>:8443
   ```

   Si una dirección no se puede abrir (la VPN apagada, por ejemplo), el host lo avisa y sigue con las demás.
3. **Un enlace de emparejamiento que incluya esa IP**: `--lan` se puede repetir, una vez por dirección alcanzable.

   ```
   host pair-code --lan <ip-lan>:8443 --lan <ip-de-la-vpn>:8443
   ```

   La identidad que comprueba es la clave fijada (`h` del enlace), nunca la IP: la VPN no es una identidad ni concede aprobaciones automáticas.
4. **Cortafuegos de Windows:** la regla es por programa. Arranca el host siempre desde el mismo `host.exe` o la regla no se aplicará.

## Cómo comprobarlo

- Con VPN activa y sin Wi‑Fi de casa, la app debe mostrar «Disponible».
- Con la VPN apagada y fuera de casa, debe mostrar el mensaje de `NoRoute` (no «gateway» ni un error genérico).
- Si el PC responde pero el servicio está parado, el mensaje es otro (`HostDown`): «El PC responde, pero el servicio está parado…».

## Lo que no hace

- No exige VPN en ninguna parte del código.
- No se aprueba nada por estar en la IP de la VPN.
- Para el error de Tailscale `401 Unauthorized` en el PC, ver `docs/troubleshooting-tailscale.md`.
