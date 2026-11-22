/// What the pairing screen says about the way the phone reaches the host. A
/// pinned link is always TLS; any other address is judged by scheme and host.
String transportNote({required String baseUrl, required bool pinned}) {
  if (pinned) {
    return 'Conexión cifrada (TLS 1.3), verificada con la huella del código.';
  }
  final uri = Uri.tryParse(baseUrl);
  if (uri == null || uri.host.isEmpty) return '';
  if (uri.scheme == 'https') return 'Conexión cifrada (HTTPS).';
  if (_loopbackHosts.contains(uri.host)) {
    return 'Sin cifrar, solo dentro de este equipo (loopback).';
  }
  return 'Sin cifrar: el tráfico viaja en claro fuera de este equipo.';
}

const Set<String> _loopbackHosts = {'localhost', '127.0.0.1', '::1'};
