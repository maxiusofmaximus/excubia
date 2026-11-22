// Control negativo: modulo valido para V1, pero sin el hook `server()`.
// Sirve para comprobar que OpenCode V1 no bloquea por si solo si el plugin
// no se carga: registra el error y continua (fail-open de carga).
export default {
  id: "control-sin-server",
  setup() {
    return {};
  },
};
