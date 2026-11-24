// The one translator from a failure to the words the operator reads. A typed refusal
// keeps its own screen where it has one; this covers what is left: JS exceptions,
// bare text from the host and refusals with no words of their own.

import { t } from "./i18n.js";

export const LOAD_FAILED = t("error.load_failed");
export const OFFLINE_TEXT = t("error.offline");
export const REJECTED_TEXT = t("error.rejected");

// «offline» is the lost socket; any other typed kind is the host answering with a refusal.
// Everything else is not something the operator can act on, so it gets the generic line.
export function friendlyError(error) {
  const kind = error && typeof error === "object" ? error.kind : undefined;
  if (kind === "offline") return OFFLINE_TEXT;
  if (typeof kind === "string") return REJECTED_TEXT;
  return LOAD_FAILED;
}
