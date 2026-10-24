import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { startMockHost } from "./mock-host.mjs";
import { startMockProvider } from "./mock-provider.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const pluginDir = process.env.E2E_PLUGIN_DIR ?? path.resolve(here, "..");
const coreRoot = path.resolve(here, "..", "..", "..");
const workRoot = process.env.E2E_WORK_ROOT ?? path.join(coreRoot, "target", "opencode-v1", "e2e");
const binary =
  process.env.OPENCODE_V1_BIN ??
  path.join(coreRoot, "target", "opencode-v1", "node_modules", "opencode-windows-x64", "bin", "opencode.exe");

const RUN_TIMEOUT_MS = Number(process.env.E2E_TIMEOUT_MS ?? 240_000);
const LOG_LEVEL = process.env.E2E_LOG_LEVEL ?? "INFO";
const PWNED = "echo PWNED > pwned.txt";

const scenarios = [
  {
    id: "aprobada",
    title: "aprobada → se ejecuta",
    hostDecision: "approved",
    ttlSeconds: 30,
    steps: [{ tool: "bash", args: { command: PWNED } }, { text: "hecho" }],
    async verify({ host, provider, workspace, log, exitCode }) {
      const pwned = await readFile(path.join(workspace, "pwned.txt"));
      const text = pwned?.replace(/^\uFEFF/, "").replace(/\0/g, "").trim();
      return [
        check(pwned !== undefined, "el comando aprobado se ejecutó (pwned.txt existe)"),
        check(text === "PWNED", "pwned.txt contiene PWNED", text),
        check(posts(host, "/v1/chat/approvals") === 1, "el plugin creó una aprobación", requests(host)),
        check(host.requests.some((item) => endsWith(item, "/consume")), "el plugin consumió la aprobación"),
        check(provider.requests.some((item) => item.hasTools && item.tools.includes("bash")), "V1 ofreció la herramienta bash al modelo"),
        check(exitCode === 0, "opencode run terminó con código 0", `exit=${exitCode}`),
        check(
          !/failed to load plugin/i.test(log),
          "el plugin V1 cargó sin error de entrada",
          log.match(/^.*failed to load plugin.*$/im)?.[0] ?? "",
        ),
      ];
    },
  },
  {
    id: "denegada",
    title: "denegada → no se ejecuta",
    hostDecision: "denied",
    ttlSeconds: 30,
    steps: [{ tool: "bash", args: { command: PWNED } }, { text: "fin" }],
    async verify({ host, workspace, log }) {
      return [
        check(!(await exists(path.join(workspace, "pwned.txt"))), "el comando denegado no se ejecutó"),
        check(posts(host, "/v1/chat/approvals") === 1, "el plugin pidió aprobación", requests(host)),
        check(!host.requests.some((item) => endsWith(item, "/consume")), "el plugin no consumió una denegación"),
        check(log.includes("[UAC] aprobación denegada por el operador"), "el error del hook llegó al run"),
      ];
    },
  },
  {
    id: "ttl",
    title: "TTL vencido → no se ejecuta",
    hostDecision: "pending",
    ttlSeconds: 2,
    pollMs: 250,
    steps: [{ tool: "bash", args: { command: PWNED } }, { text: "fin" }],
    async verify({ host, workspace, log }) {
      return [
        check(!(await exists(path.join(workspace, "pwned.txt"))), "el comando no se ejecutó tras vencer el TTL"),
        check(posts(host, "/v1/chat/approvals") === 1, "el plugin pidió aprobación", requests(host)),
        check(host.polls() >= 1, "el plugin sondeó la decisión", `polls=${host.polls()}`),
        check(log.includes("[UAC] aprobación expirada tras 2 s"), "el run registró la expiración del TTL"),
      ];
    },
  },
  {
    id: "host-caido",
    title: "servidor caído → falla cerrado",
    hostUrl: "http://127.0.0.1:1",
    ttlSeconds: 5,
    steps: [{ tool: "bash", args: { command: PWNED } }, { text: "fin" }],
    async verify({ workspace, log }) {
      return [
        check(!(await exists(path.join(workspace, "pwned.txt"))), "sin host no se ejecutó el comando (fail closed)"),
        check(log.includes("[UAC] el host de aprobaciones no responde"), "el run registró el host caído como bloqueo"),
      ];
    },
  },
  {
    id: "dentro-de-politica",
    title: "lectura/escritura dentro del cwd → sin petición",
    hostDecision: "pending",
    ttlSeconds: 5,
    steps: [
      { tool: "read", args: { filePath: "@workspace/inside.txt" } },
      { tool: "write", args: { filePath: "@workspace/out.txt", content: "escrito" } },
      { text: "hecho" },
    ],
    async verify({ host, workspace, log, exitCode }) {
      const out = await readFile(path.join(workspace, "out.txt"));
      return [
        check(host.requests.length === 0, "el plugin no pidió aprobación para acciones dentro de la política", requests(host)),
        check(out === "escrito", "la escritura dentro del cwd se ejecutó", out),
        check(log.includes("inside.txt"), "la lectura dentro del cwd devolvió contenido"),
        check(exitCode === 0, "opencode run terminó con código 0", `exit=${exitCode}`),
      ];
    },
  },
];

function check(ok, label, detail) {
  return { ok: Boolean(ok), label, detail: detail === undefined ? "" : String(detail).slice(0, 200) };
}

function posts(host, url) {
  return host.requests.filter((item) => item.method === "POST" && item.url === url).length;
}

function endsWith(item, suffix) {
  return item.url?.endsWith(suffix);
}

function requests(host) {
  return host.requests.map((item) => `${item.method} ${item.url}`).join(", ") || "(sin peticiones)";
}

async function exists(target) {
  try {
    await fs.access(target);
    return true;
  } catch {
    return false;
  }
}

async function readFile(target) {
  try {
    const buffer = await fs.readFile(target);
    return buffer[0] === 0xff && buffer[1] === 0xfe ? buffer.toString("utf16le") : buffer.toString("utf8");
  } catch {
    return undefined;
  }
}

function baseEnv() {
  const env = { ...process.env };
  for (const key of Object.keys(env)) {
    if (/^(OPENCODE|XDG_|HN_)/i.test(key)) delete env[key];
  }
  return env;
}

async function prepare(workspace, config) {
  await fs.rm(workspace, { recursive: true, force: true });
  await fs.mkdir(workspace, { recursive: true });
  await fs.writeFile(path.join(workspace, "inside.txt"), "contenido dentro del cwd", "utf8");
  await fs.writeFile(path.join(workspace, "opencode.json"), JSON.stringify(config, null, 2), "utf8");
}

async function runOpencode({ workspace, configPath, env }) {
  return new Promise((resolve) => {
    const args = ["run", "--print-logs", "--log-level", LOG_LEVEL, "--format", "json"];
    if (process.env.E2E_PURE) args.push("--pure");
    args.push("--model", "mock/mock", "--agent", "build", "--dir", workspace, "haz la tarea");
    const child = spawn(binary, args, { cwd: workspace, env, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill();
    }, RUN_TIMEOUT_MS);
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr, timedOut });
    });
  });
}

async function runScenario(scenario) {
  const dir = path.join(workRoot, scenario.id);
  const workspace = path.join(dir, "workspace");
  const xdg = (name) => path.join(dir, `xdg-${name}`);
  const host = scenario.hostUrl ? undefined : await startMockHost({ decision: scenario.hostDecision });
  const provider = await startMockProvider({
    steps: scenario.steps.map((step) =>
      step.tool
        ? { ...step, args: replaceWorkspace(step.args, workspace) }
        : step,
    ),
  });
  const config = {
    $schema: "https://opencode.ai/config.json",
    autoupdate: false,
    share: "disabled",
    plugin: [pluginDir],
    permission: { bash: "allow", edit: "allow", read: "allow", webfetch: "allow" },
    provider: {
      mock: {
        npm: "@ai-sdk/openai-compatible",
        name: "Mock",
        options: { baseURL: provider.url, apiKey: "dummy" },
        models: { mock: { name: "Mock", tool_call: true, limit: { context: 200000, output: 8192 } } },
      },
    },
    model: "mock/mock",
  };
  await prepare(workspace, config);
  const configPath = path.join(workspace, "opencode.json");
  const env = {
    ...baseEnv(),
    HOME: path.join(dir, "home"),
    USERPROFILE: path.join(dir, "home"),
    OPENCODE_CONFIG: configPath,
    OPENCODE_DISABLE_PROJECT_CONFIG: "1",
    XDG_CONFIG_HOME: xdg("config"),
    XDG_DATA_HOME: xdg("data"),
    XDG_CACHE_HOME: xdg("cache"),
    XDG_STATE_HOME: xdg("state"),
    HN_HOOK_URL: host?.url ?? scenario.hostUrl,
    HN_HOOK_TOKEN: host?.token ?? "token-e2e",
    HN_PANE_ID: "w1:p9",
    HN_UAC_TTL_SECONDS: String(scenario.ttlSeconds ?? 30),
    HN_UAC_POLL_MS: String(scenario.pollMs ?? 250),
  };
  const started = Date.now();
  const run = await runOpencode({ workspace, configPath, env });
  const elapsedMs = Date.now() - started;
  const callLines = provider.requests
    .map((item) => `model=${item.model} tools=${item.tools.length} step=${item.step}`)
    .join("\n");
  const hostLines = host
    ? host.requests
        .map(
          (item) =>
            `${item.method} ${item.url} token=${item.authorized ? "ok" : "NO"}${item.body ? ` body=${JSON.stringify(item.body)}` : ""}`,
        )
        .join("\n")
    : `(sin host: ${scenario.hostUrl})`;
  const log = [
    `# caso ${scenario.id} (${scenario.title})`,
    `binario: ${binary}`,
    `exit=${run.code} timedOut=${run.timedOut} elapsedMs=${elapsedMs}`,
    "# llamadas al modelo simulado",
    callLines || "(sin llamadas)",
    "# peticiones al host de aprobaciones",
    hostLines,
    "# stdout",
    run.stdout,
    "# stderr",
    run.stderr,
  ].join("\n");
  await fs.writeFile(path.join(dir, "run.log"), log, "utf8");

  const checks = await scenario.verify({
    host: host ?? { requests: [], polls: () => 0 },
    provider,
    workspace,
    log,
    exitCode: run.code,
  });
  await provider.close();
  if (host) await host.close();
  return { scenario, checks, elapsedMs, dir };
}

function replaceWorkspace(args, workspace) {
  return Object.fromEntries(
    Object.entries(args ?? {}).map(([key, value]) => [
      key,
      typeof value === "string" ? value.replace("@workspace", workspace) : value,
    ]),
  );
}

async function main() {
  const only = process.argv.slice(2).filter((arg) => !arg.startsWith("-"));
  const selected = scenarios.filter((scenario) => only.length === 0 || only.includes(scenario.id));
  if (selected.length === 0) {
    console.error(`Sin casos que ejecutar. Disponibles: ${scenarios.map((item) => item.id).join(", ")}`);
    process.exit(2);
  }
  console.log(`Binario V1: ${binary}`);
  let failed = 0;
  for (const scenario of selected) {
    const result = await runScenario(scenario);
    console.log(`\n== ${scenario.id}: ${scenario.title} (${result.elapsedMs} ms) — ${result.dir}\\run.log`);
    for (const item of result.checks) {
      console.log(`  ${item.ok ? "PASS" : "FAIL"} ${item.label}${item.ok ? "" : ` — ${item.detail}`}`);
      if (!item.ok) failed++;
    }
  }
  console.log(failed === 0 ? "\nTODO EN VERDE" : `\n${failed} comprobaciones en rojo`);
  process.exit(failed === 0 ? 0 : 1);
}

await main();
