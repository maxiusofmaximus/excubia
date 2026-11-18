// The Agenda destination: calendar, notes and contacts, one sidebar entry with a
// tab bar inside each of the three screens. Each screen loads its own data.

import { installContacts, openContacts } from "./agenda-contacts.js";
import { installNotes, openNotes } from "./agenda-notes.js";
import { installCalendar, openCalendar } from "./calendar.js";
import { installAlerts } from "./alerts.js";

const OPENERS = { calendar: openCalendar, notes: openNotes, contacts: openContacts };

export function openAgenda(tab = "calendar") {
  (OPENERS[tab] || openCalendar)();
}

export function installAgenda() {
  installContacts();
  installNotes();
  installCalendar();
  installAlerts();
  document.addEventListener("click", (event) => {
    const tab = event.target.closest(".agenda-tabs [data-agenda]");
    if (tab) openAgenda(tab.dataset.agenda);
  });
}
