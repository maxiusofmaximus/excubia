// Etiquetas del catálogo de oficios (team_roles) por ID estable. El host entrega el texto en español:
// en español se muestra tal cual; en inglés se usa la traducción de su ID (claves catalog.area.* y
// catalog.role.*, fragmento C3). Un ID desconocido conserva la etiqueta original. No muta sus argumentos.
import { dictionaries, lang, t } from "./i18n.js";

export function catalogLabel(kind, id, host) {
  const key = `catalog.${kind}.${id}`;
  if (lang() !== "en" || !Object.hasOwn(dictionaries.en, key)) return host;
  return t(key);
}

export function areaLabel(area) {
  return catalogLabel("area", area?.id, area?.label);
}

export function roleLabel(role) {
  return catalogLabel("role", role?.id, role?.title);
}
