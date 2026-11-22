/// Steps for a maker that stops background services (RFC-PUSH-OTROS §3.1, TECNO/HiOS).
/// The app shows them as text; it never changes a setting.
const List<String> hiosSteps = [
  '1. Pulsa «Abrir inicio automático» y deja esta app en «Permitido».',
  '2. Batería de esta app: «Sin restricciones».',
  '3. Tras reiniciar el móvil, escribe tu PIN: los avisos vuelven solos unos segundos después.',
];

/// The steps for [manufacturer] (lower case, as Build.MANUFACTURER gives it), or null for none.
List<String>? manufacturerSteps(String manufacturer) =>
    const {'tecno', 'infinix', 'itel'}.contains(manufacturer) ? hiosSteps : null;
