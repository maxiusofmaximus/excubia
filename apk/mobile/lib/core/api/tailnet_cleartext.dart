/// Cleartext HTTP is only allowed towards hosts the transport could plausibly
/// own: the tailnet itself, a Tailscale name, or the loopback/emulator
/// addresses a development setup uses. Mirrors `TailnetCleartext` in the
/// Android app, so a Flutter build cannot send in the clear where the native
/// build refuses.
class TailnetCleartext {
  TailnetCleartext._();

  static bool isAllowedHost(String raw) {
    var host = raw.trim().toLowerCase();
    if (host.endsWith('.')) host = host.substring(0, host.length - 1);
    if (host.startsWith('[')) host = host.substring(1);
    if (host.endsWith(']')) host = host.substring(0, host.length - 1);
    if (host.isEmpty) return false;
    if (host == 'localhost' || host == '10.0.2.2') return true;
    if (_isLoopback(host)) return true;
    if (host.endsWith('.ts.net')) return true;
    if (_isTailnetIpv4(host)) return true;
    if (!host.contains('.')) return true;
    return false;
  }

  static bool isCleartextAllowed(Uri url) =>
      url.scheme != 'http' || isAllowedHost(url.host);

  static bool _isLoopback(String host) =>
      host == '::1' || host.startsWith('127.');

  static bool _isTailnetIpv4(String host) {
    final parts = host.split('.');
    if (parts.length != 4) return false;
    final octets = <int>[];
    for (final part in parts) {
      final value = int.tryParse(part);
      if (value == null || value < 0 || value > 255) return false;
      octets.add(value);
    }
    return octets[0] == 100 && octets[1] >= 64 && octets[1] <= 127;
  }
}
