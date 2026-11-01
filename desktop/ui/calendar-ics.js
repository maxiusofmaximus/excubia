// «Importar .ics» and «Exportar .ics» of «Calendario». The shell opens the dialogs and reads or
// writes the file; this screen shows what came back and reloads the list after an import.

import { $, invoke, setBusy } from "./core.js";
import { element } from "./chatdom.js";
import { reportWrite } from "./agenda-ui.js";
import { icsErrorText, importSummary, savedText, skippedLines, skippedNote } from "./calendar-ics-model.js";

export function installIcs({ visibleWindow, refresh }) {
  const result = $("calendar-ics-result");
  const summary = $("calendar-ics-summary");
  const problem = $("calendar-ics-problem");
  const skipped = $("calendar-ics-skipped");
  const skippedList = $("calendar-ics-skipped-list");
  const more = $("calendar-ics-more");

  function reset() {
    result.hidden = true;
    summary.textContent = "";
    problem.hidden = true;
    skipped.hidden = true;
    skippedList.replaceChildren();
    more.hidden = true;
  }

  function fail(failure, permission, operation) {
    reportWrite(failure, {
      fields: {},
      problem,
      permission,
      words: (cause) => icsErrorText(cause, operation),
    });
    result.hidden = false;
  }

  function showImport(outcome) {
    summary.textContent = importSummary(outcome);
    skippedList.replaceChildren(...skippedLines(outcome).map((line) => element("li", null, line)));
    skipped.hidden = outcome.details.length === 0;
    more.textContent = skippedNote(outcome) ?? "";
    more.hidden = !more.textContent;
    result.hidden = false;
  }

  async function importFile(button) {
    reset();
    setBusy(button, true, "Importando…");
    try {
      const outcome = await invoke("calendar_import_ics");
      if (!outcome) return;
      showImport(outcome);
      refresh();
    } catch (failure) {
      fail(failure, "RESPOND", "import");
    } finally {
      setBusy(button, false);
    }
  }

  async function exportFile(button) {
    reset();
    const scope = $("calendar-export-scope").value;
    const range = scope === "all" ? { from: null, to: null } : visibleWindow();
    setBusy(button, true, "Exportando…");
    try {
      const name = await invoke("calendar_export_ics", range);
      if (!name) return;
      summary.textContent = savedText(name);
      result.hidden = false;
    } catch (failure) {
      fail(failure, "READ_STATE", "export");
    } finally {
      setBusy(button, false);
    }
  }

  $("btn-calendar-import").addEventListener("click", (event) => importFile(event.currentTarget));
  $("btn-calendar-export").addEventListener("click", (event) => exportFile(event.currentTarget));
}
