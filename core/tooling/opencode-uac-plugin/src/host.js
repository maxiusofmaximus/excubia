import { randomUUID } from "node:crypto";
import { redact, redactText } from "./digest.js";

const DEFAULT_TTL_SECONDS = 120;
const MAX_TTL_SECONDS = 300;
const REQUEST_TIMEOUT_MS = 5000;
const DEFAULT_POLL_MS = 750;
const LOOPBACK_HOSTS = new Set(["127.0.0.1", "::1", "[::1]", "localhost"]);

export class UacDenied extends Error {
  constructor(message, code = "approval_denied") {
    super(message);
    this.name = "UacDenied";
    this.code = code;
  }
}

function integer(value, fallback, min, max) {
  const parsed = Number.parseInt(String(value ?? ""), 10);
  if (!Number.isFinite(parsed)) return fallback;
  return Math.min(max, Math.max(min, parsed));
}

function hostUrl(env) {
  const raw = String(env.HN_HOOK_URL ?? "").trim().replace(/\/+$/, "");
  if (!raw) return undefined;
  let url;
  try {
    url = new URL(raw);
  } catch {
    throw new UacDenied("HN_HOOK_URL no es una URL válida; se deniega por seguridad", "uac_unconfigured");
  }
  if (!LOOPBACK_HOSTS.has(url.hostname) || url.username || url.password) {
    throw new UacDenied("HN_HOOK_URL debe ser loopback; se deniega por seguridad", "uac_not_loopback");
  }
  return url;
}

/** Configuración efectiva. Devuelve null si falta algo: el llamador debe denegar. */
export function hostConfig(env = process.env) {
  const url = hostUrl(env);
  const token = String(env.HN_HOOK_TOKEN ?? "").trim();
  if (!url || !token) return null;
  return {
    base: url.href.replace(/\/+$/, ""),
    token,
    ttlSeconds: integer(env.HN_UAC_TTL_SECONDS, DEFAULT_TTL_SECONDS, 1, MAX_TTL_SECONDS),
    pollMs: integer(env.HN_UAC_POLL_MS, DEFAULT_POLL_MS, 50, 5000),
    paneId: String(env.HN_PANE_ID ?? "").trim(),
    runId: String(env.HN_RUN_ID ?? "").trim(),
  };
}

function remaining(deadline, now) {
  return Math.max(1, Math.min(REQUEST_TIMEOUT_MS, deadline - now()));
}

async function callJson(fetchImpl, url, options, deadline, now, allowed) {
  let response;
  try {
    response = await fetchImpl(url, { ...options, signal: AbortSignal.timeout(remaining(deadline, now)) });
  } catch {
    throw new UacDenied("el host de aprobaciones no responde; se deniega por seguridad", "approval_host_unreachable");
  }
  if (!allowed.includes(response.status)) {
    throw new UacDenied(`el host de aprobaciones respondió HTTP ${response.status}; se deniega`, "approval_host_error");
  }
  if (response.status === 204) return {};
  try {
    return await response.json();
  } catch {
    throw new UacDenied("respuesta inválida del host de aprobaciones; se deniega", "approval_host_error");
  }
}

function createBody({ descriptor, digest, event, cwd, config }) {
  const payload = event.input && typeof event.input === "object" ? event.input : { value: event.input ?? null };
  return {
    client_request_id: randomUUID(),
    pane_id: config.paneId || null,
    agent_id: event.agent ?? null,
    run_id: config.runId || null,
    session_id: event.sessionID ?? null,
    tool: event.tool,
    kind: descriptor.kind,
    summary: redactText(descriptor.summary),
    cwd,
    paths: descriptor.paths,
    action_digest: digest,
    payload_redacted: redact(payload),
    ttl_seconds: config.ttlSeconds,
  };
}

function decisionOf(state) {
  if (state?.status === "approved") return "approved";
  if (state?.status === "denied") return "denied";
  if (state?.status === "expired") return "expired";
  if (state?.status === "consumed") return "consumed";
  if (state?.status === "pending") return "pending";
  return "invalid";
}

/**
 * Crea la aprobación, espera la decisión y la consume antes de dejar ejecutar.
 * Cualquier error, respuesta rara o timeout termina en UacDenied (fail closed).
 */
export async function requestApproval({
  descriptor,
  digest,
  event,
  cwd,
  env = process.env,
  fetchImpl = globalThis.fetch,
  now = Date.now,
  sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
}) {
  const config = hostConfig(env);
  if (!config) throw new UacDenied("UAC no configurada (HN_HOOK_URL/HN_HOOK_TOKEN); se deniega", "uac_unconfigured");

  const headers = {
    "content-type": "application/json",
    accept: "application/json",
    "x-hn-hook-token": config.token,
  };
  const deadline = now() + config.ttlSeconds * 1000;
  const created = await callJson(
    fetchImpl,
    `${config.base}/v1/chat/approvals`,
    { method: "POST", headers, body: JSON.stringify(createBody({ descriptor, digest, event, cwd, config })) },
    deadline,
    now,
    [200, 201],
  );
  const approvalId = typeof created?.approval_id === "string" ? created.approval_id : "";
  if (!approvalId) throw new UacDenied("el host no devolvió approval_id; se deniega", "approval_host_error");

  const serverDeadline = Date.parse(String(created.expires_at ?? ""));
  const effectiveDeadline = Number.isFinite(serverDeadline) ? Math.min(deadline, serverDeadline) : deadline;

  while (now() < effectiveDeadline) {
    await sleep(config.pollMs);
    // No lanzar un GET que no quepa en el presupuesto: sería un falso "host caído".
    if (effectiveDeadline - now() <= config.pollMs) break;
    const state = await callJson(
      fetchImpl,
      `${config.base}/v1/chat/approvals/${encodeURIComponent(approvalId)}`,
      { method: "GET", headers: { accept: "application/json", "x-hn-hook-token": config.token } },
      effectiveDeadline,
      now,
      [200],
    );
    const decision = decisionOf(state);
    if (decision === "pending") continue;
    if (decision === "approved") {
      await callJson(
        fetchImpl,
        `${config.base}/v1/chat/approvals/${encodeURIComponent(approvalId)}/consume`,
        { method: "POST", headers, body: "{}" },
        effectiveDeadline,
        now,
        [200, 204],
      );
      return { approvalId, digest };
    }
    if (decision === "denied") throw new UacDenied("aprobación denegada por el operador", "approval_denied");
    if (decision === "expired") throw new UacDenied("aprobación expirada (TTL)", "approval_expired");
    if (decision === "consumed") throw new UacDenied("aprobación ya consumida", "approval_already_decided");
    throw new UacDenied("estado de aprobación desconocido; se deniega", "approval_host_error");
  }
  throw new UacDenied(`aprobación expirada tras ${config.ttlSeconds} s (TTL)`, "approval_expired");
}
