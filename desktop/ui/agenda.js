// Installs the agenda screens (contacts, notes, calendar, alerts). Their entry points
// are the sidebar destinations in ui/nav.js; each screen loads its data when it opens.

import { installContacts } from "./agenda-contacts.js";
import { installNotes } from "./agenda-notes.js";
import { installCalendar } from "./calendar.js";
import { installAlerts } from "./alerts.js";

export function installAgenda() {
  installContacts();
  installNotes();
  installCalendar();
  installAlerts();
}
