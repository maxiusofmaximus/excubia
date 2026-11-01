// The agenda's entry points: the «Contactos», «Notas», «Calendario» and «Avisos» tabs of every top bar,
// and the install of the screens. The screens load their own data when they are opened.

import { $, showView } from "./core.js";
import { installContacts, openContacts } from "./agenda-contacts.js";
import { installNotes, openNotes } from "./agenda-notes.js";
import { installCalendar, openCalendar } from "./calendar.js";
import { installAlerts, openAlerts } from "./alerts.js";

export function installAgenda() {
  installContacts();
  installNotes();
  installCalendar();
  installAlerts();

  $("tab-contacts").addEventListener("click", openContacts);
  $("tab-notes").addEventListener("click", openNotes);
  $("tab-calendar").addEventListener("click", openCalendar);
  $("tab-alerts").addEventListener("click", openAlerts);
  $("team-tab-contacts").addEventListener("click", openContacts);
  $("team-tab-notes").addEventListener("click", openNotes);
  $("team-tab-calendar").addEventListener("click", openCalendar);
  $("team-tab-alerts").addEventListener("click", openAlerts);

  $("contacts-tab-chat").addEventListener("click", () => showView("chat"));
  $("contacts-tab-team").addEventListener("click", () => showView("agents"));
  $("contacts-tab-notes").addEventListener("click", openNotes);
  $("contacts-tab-calendar").addEventListener("click", openCalendar);
  $("contacts-tab-alerts").addEventListener("click", openAlerts);
  $("notes-tab-chat").addEventListener("click", () => showView("chat"));
  $("notes-tab-team").addEventListener("click", () => showView("agents"));
  $("notes-tab-contacts").addEventListener("click", openContacts);
  $("notes-tab-calendar").addEventListener("click", openCalendar);
  $("notes-tab-alerts").addEventListener("click", openAlerts);

  $("calendar-tab-chat").addEventListener("click", () => showView("chat"));
  $("calendar-tab-team").addEventListener("click", () => showView("agents"));
  $("calendar-tab-contacts").addEventListener("click", openContacts);
  $("calendar-tab-notes").addEventListener("click", openNotes);
  $("calendar-tab-alerts").addEventListener("click", openAlerts);

  $("alerts-tab-chat").addEventListener("click", () => showView("chat"));
  $("alerts-tab-team").addEventListener("click", () => showView("agents"));
  $("alerts-tab-contacts").addEventListener("click", openContacts);
  $("alerts-tab-notes").addEventListener("click", openNotes);
  $("alerts-tab-calendar").addEventListener("click", openCalendar);
}
