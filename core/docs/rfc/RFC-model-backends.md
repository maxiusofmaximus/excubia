# RFC: backends de modelo intercambiables

Estado: aprobado · 2026-10-23 · Origen: spike STRATA1 fase 1 · **[V]** verificado (ruta:línea) · **[I]** inferencia · **[NV]** no verificado

## 1. Problema
Hay que consumir varios motores de inferencia (de pago y locales) con una sola interfaz. Sin modelos de pago, el sistema cae a un backend local. El puntuador ya habla con un `local_http` OpenAI-compatible: `POST {endpoint}/v1/chat/completions`, solo loopback, con `timeout_ms = 200` y fail-closed [V: `core/docs/rfc/approvals-escalation.md:89-97`, `core/docs/contracts/scorer.md:18,62`].

## 2. Interfaz mínima
```rust
pub trait ModelBackend: Send + Sync {
    fn id(&self) -> &BackendId;
    fn caps(&self) -> &Capabilities;                 // fijas al registrar
    async fn health(&self) -> Health;                // ok | loading | down; sin inferencia
    async fn complete(&self, req: &ChatRequest, budget: Duration)
        -> Result<Completion, BackendError>;         // budget = deadline duro
}

pub struct Capabilities {
    pub chat_completions: bool,
    pub streaming: bool,
    pub max_context_tokens: u32,
    pub max_output_tokens: u32,
    pub latency_p50_ms: Option<u32>,                 // medida en esta máquina, no declarada
    pub cost: Cost,                                  // Free | Quota | PerToken
    pub requires_network: bool,
    pub auth: Auth,                                  // None | BearerEnv("NOMBRE_VAR")
}
```
Errores con `thiserror`: `Timeout`, `Unreachable`, `Unavailable` (5xx o carga), `Unauthorized`, `ContextTooLarge`, `InvalidResponse`. Sin `unwrap()` en rutas reales.

## 3. Registro y selección
```toml
[backends.clef]    kind = "openai_compat"  endpoint = "http://127.0.0.1:8765"  cost = "free"
[backends.strata]  kind = "openai_compat"  endpoint = "http://127.0.0.1:8080"  cost = "free"  auth = "bearer_env"  api_key_env = "STRATA_LOCAL_KEY"
[routing]
chat = ["anthropic", "strata"]          # orden de preferencia
allow_paid = false                      # sin modelos de pago -> solo backends cost = "free"
```
- `select(req)`: filtra por `allow_paid`, `health != down`, y capacidades requeridas (p. ej. `streaming`); respeta el orden.
- **Fallback** solo ante `Timeout | Unreachable | Unavailable`. Nunca ante `Unauthorized`, `ContextTooLarge` o `InvalidResponse` (se reportan).
- `loading` no recibe peticiones; el registro reintenta `health` con backoff, sin bucle de sondeo continuo.

## 4. Encaje del puntuador
- Su backend `local_http` pasa a ser `kind = "openai_compat"` con `loopback_only = true`. El `timeout_ms` del puntuador sigue siendo su presupuesto; el registro lo impone como `budget`. Fail-closed sin cambios.
- **[I] Strata no cumple 200 ms para el puntuador**: con `max_tokens: 32` y 94 tok/s (spike) la sola generación ya tarda ~340 ms, antes del prefill. Strata sirve como fallback general, no como puntuador. Clef-flash (~39 ms según Cloudflare, [NV]) sí es candidato.

## 5. Añadir un backend nuevo
1. Un módulo `backends/<kind>.rs` (<400 líneas) que implemente `ModelBackend`, y una entrada `[backends.<id>]` en TOML; sin tocar núcleo ni enrutado.
2. Pruebas de contrato compartidas (tabla de casos: timeout, 401, 5xx, respuesta mala, streaming) contra un servidor simulado; y una prueba de `health` real.

## 6. Strata como backend `openai_compat` (datos verificados)
- `GET /health` no pide clave y devuelve `api_key: bool`, `max_context`, `loaded` [V: `serve/server.py:6422-6426`].
- `POST /v1/chat/completions` con `Authorization: Bearer` o `x-api-key`; streaming SSE [V: `server.py:6548,6876`].
- Carga inicial declarada ~270 s [spike, no verificado en esta fase]: el registro lo marca `loading` hasta que `/health` dé `loaded: true`.
- Clave solo por variable de entorno (`api_key_env`); nunca en TOML [V: el contrato del puntuador exige nombres de variable, `scorer.md:32`].

## 7. Decisiones abiertas
1. **Motor local o no:** [I] recomiendo Strata solo como fallback "sin pago" (latencia de segundos, no de milisegundos) y no en el puntuador.
2. **Licencia:** [NV] sin licencia verificada del motor ni de los pesos; no redistribuir hasta confirmarla.
3. **Fallback en `Unauthorized`:** recomiendo NO caer a otro backend (oculta errores de configuración).
