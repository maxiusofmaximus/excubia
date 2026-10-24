// Reenviador TCP en espacio de usuario para la prueba de emparejamiento por red.
// Escucha SOLO en la IP de la tailnet y reenvía cada conexión a loopback, donde
// corre el host del arnés. Sin dependencias. Se apaga solo a los 30 min.
//
//   node tailnet-forward.mjs <ip-tailnet> [puerto]

import net from 'node:net';

const LISTEN_HOST = process.argv[2];
if (!LISTEN_HOST) {
  console.error('uso: node tailnet-forward.mjs <ip-tailnet> [puerto]');
  process.exit(2);
}
const PORT = Number(process.argv[3] ?? 18878);
const TARGET_HOST = '127.0.0.1';
const MAX_LIFETIME_MS = 30 * 60 * 1000;

const sockets = new Set();

function track(socket) {
  sockets.add(socket);
  socket.on('close', () => sockets.delete(socket));
}

const server = net.createServer((client) => {
  const upstream = net.connect(PORT, TARGET_HOST);
  track(client);
  track(upstream);
  // Cerrar un extremo cierra el otro: no quedan medias conexiones.
  for (const [from, to] of [[client, upstream], [upstream, client]]) {
    from.pipe(to);
    from.on('end', () => to.end());
    from.on('error', () => { client.destroy(); upstream.destroy(); });
  }
});

function shutdown(reason) {
  console.log(`reenviador parado: ${reason}`);
  server.close();
  for (const socket of sockets) socket.destroy();
  process.exit(0);
}

server.on('error', (error) => {
  console.error(`no se pudo escuchar en ${LISTEN_HOST}:${PORT}: ${error.message}`);
  process.exit(1);
});

server.listen(PORT, LISTEN_HOST, () => {
  console.log(`reenviador escuchando en ${LISTEN_HOST}:${PORT} -> ${TARGET_HOST}:${PORT} (30 min)`);
  setTimeout(() => shutdown('vida máxima de 30 min'), MAX_LIFETIME_MS).unref();
});

process.on('SIGTERM', () => shutdown('SIGTERM'));
process.on('SIGINT', () => shutdown('SIGINT'));
