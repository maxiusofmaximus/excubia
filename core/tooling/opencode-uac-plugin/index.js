import { classify, extraTools } from "./src/policy.js";
import { actionDigest } from "./src/digest.js";
import { requestApproval, UacDenied } from "./src/host.js";

export const PLUGIN_ID = "herdr.uac";

function deny(error) {
  const message = error instanceof UacDenied ? error.message : "error interno del guardián UAC";
  return new Error(`[UAC] ${message}. Acción denegada (fail closed).`);
}

/**
 * Núcleo común V1/V2. No conoce el SDK: recibe un evento normalizado y lanza
 * si la acción debe bloquearse. Cualquier fallo termina en bloqueo.
 */
export async function guard(event, { cwd, env = process.env } = {}) {
  try {
    const directory = cwd || process.cwd();
    const descriptor = classify(event?.tool, event?.input, directory, extraTools(env));
    if (!descriptor) return;
    const digest = actionDigest({
      kind: descriptor.kind,
      tool: event.tool,
      args: event.input,
      cwd: directory,
      paths: descriptor.paths,
    });
    await requestApproval({ descriptor, digest, event, cwd: directory, env });
  } catch (error) {
    throw deny(error);
  }
}

const plugin = {
  id: PLUGIN_ID,

  async setup(ctx) {
    const cwd = ctx?.location?.directory ?? process.cwd();
    const registration = await ctx.tool.hook("execute.before", (event) =>
      guard(
        { tool: event.tool, input: event.input, sessionID: event.sessionID, agent: event.agent, id: event.id },
        { cwd },
      ),
    );
    return () => registration.dispose();
  },

  // Compatibilidad con OpenCode V1, que llama a server() y usa hooks por clave.
  async server(input) {
    const cwd = input?.directory ?? process.cwd();
    return {
      "tool.execute.before": async (event, output) => {
        await guard(
          { tool: event.tool, input: output?.args, sessionID: event.sessionID, agent: undefined, id: event.callID },
          { cwd },
        );
      },
    };
  },
};

export default plugin;
