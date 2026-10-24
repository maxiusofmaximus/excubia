import http from "node:http";

const TTL_MS = 60_000;

/**
 * Host de aprobaciones simulado según INTERFACE.md.
 * `decision` decide qué responde el sondeo `GET /v1/chat/approvals/{id}`:
 * "approved" | "denied" | "pending" (nunca decide, para el caso TTL).
 */
export function startMockHost({ decision, token = "token-e2e" }) {
  const requests = [];
  const server = http.createServer((req, res) => {
    collect(req).then((raw) => {
      const body = raw ? JSON.parse(raw) : undefined;
      const authorized = req.headers["x-hn-hook-token"] === token;
      const entry = { method: req.method, url: req.url, authorized, body };
      requests.push(entry);
      if (!authorized) return send(res, 401, { code: "forbidden" });
      if (req.method === "POST" && req.url === "/v1/chat/approvals") {
        return send(res, 201, {
          approval_id: "apr_e2e",
          status: "pending",
          expires_at: new Date(Date.now() + TTL_MS).toISOString(),
        });
      }
      if (req.method === "POST" && req.url === "/v1/chat/approvals/apr_e2e/consume") {
        return send(res, 200, { approval_id: "apr_e2e", status: "consumed" });
      }
      if (req.method === "GET" && req.url === "/v1/chat/approvals/apr_e2e") {
        return send(res, 200, {
          approval_id: "apr_e2e",
          status: decision,
          expires_at: new Date(Date.now() + TTL_MS).toISOString(),
        });
      }
      return send(res, 404, { code: "no_such_approval" });
    });
  });

  return listen(server).then((port) => ({
    url: `http://127.0.0.1:${port}`,
    token,
    requests,
    polls: () => requests.filter((item) => item.method === "GET").length,
    close: () => new Promise((resolve) => server.close(resolve)),
  }));
}

function collect(req) {
  return new Promise((resolve) => {
    let raw = "";
    req.on("data", (chunk) => (raw += chunk));
    req.on("end", () => resolve(raw));
  });
}

function send(res, status, body) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

function listen(server) {
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}
