// Saving the operator's edit of a draft. The host closes the send open on it, so
// the send flow of the draft starts over and the draft is read again. The host
// call is injected, so the tests can run without the screen.

import { errorMessage } from "./format.js";
import { t } from "./i18n.js";

export const SAVED_TEXT = t("mail.edit.saved");

export function createDraftEditor({ invoke, forget }) {
  return async function save(account, draftId, { subject, body }, reload) {
    try {
      await invoke("mail_draft_update", { account, draftId, subject, body });
    } catch (error) {
      return { ok: false, text: errorMessage(error) };
    }
    forget(draftId);
    await reload();
    return { ok: true, text: SAVED_TEXT };
  };
}
