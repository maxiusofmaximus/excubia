/// Address helpers shared by the manual pairing field and the QR flow.
///
/// `normaliseUrl` guesses a scheme for a bare host, because a phone keyboard
/// does not reliably produce `:` or `/`.
class PairingAddress {
  PairingAddress._();

  static final _scheme = RegExp(r'^https?://', caseSensitive: false);
  static final _ipv4 = RegExp(r'^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$');

  /// A scheme the user typed is kept. A bare loopback or tailnet (100.64.0.0/10)
  /// host gets `http://`, the development path and the tailnet, where the
  /// transport is already private. Everything else, private addresses included,
  /// gets `https://`: a private address has no certificate to check, so it is
  /// never guessed as plain http.
  static String normaliseUrl(String raw) {
    final trimmed = raw.trim().replaceAll(RegExp(r'/+$'), '');
    if (trimmed.isEmpty) return '';
    if (_scheme.hasMatch(trimmed)) return trimmed;
    final host = _hostOf(trimmed);
    final plain = _isLoopback(host) || _isTailnet(host);
    return '${plain ? 'http' : 'https'}://$trimmed';
  }

  /// The host of a scheme-less address: `[v6]` keeps its brackets out, and
  /// otherwise the host ends at the first `:` (port) or `/` (path).
  static String _hostOf(String address) {
    if (address.startsWith('[')) {
      final end = address.indexOf(']');
      return end < 0 ? address : address.substring(1, end);
    }
    return address.split(RegExp(r'[:/]')).first;
  }

  /// The four octets of a dotted IPv4 host, or null for anything else.
  static List<int>? _octets(String host) {
    final match = _ipv4.firstMatch(host.toLowerCase());
    if (match == null) return null;
    final octets = [for (var i = 1; i <= 4; i++) int.parse(match.group(i)!)];
    return octets.any((o) => o > 255) ? null : octets;
  }

  static bool _isLoopback(String host) {
    final h = host.toLowerCase();
    if (h == 'localhost' || h == '::1') return true;
    return _octets(h)?.first == 127;
  }

  static bool _isTailnet(String host) {
    final o = _octets(host);
    return o != null && o[0] == 100 && o[1] >= 64 && o[1] <= 127;
  }

  /// RFC 1918: 10/8, 172.16/12 and 192.168/16.
  static bool _isPrivate(String host) {
    final o = _octets(host);
    if (o == null) return false;
    final (a, b) = (o[0], o[1]);
    return a == 10 || (a == 172 && b >= 16 && b <= 31) || (a == 192 && b == 168);
  }

  static bool isLoopbackUrl(String url) {
    final host = Uri.tryParse(url)?.host.toLowerCase();
    return host == '127.0.0.1' || host == 'localhost' || host == '::1';
  }

  /// Why a loopback address is refused, or null when it is acceptable. Release
  /// builds must not accept it: on a phone it points at the phone itself.
  static String? loopbackProblem(String url, {required bool allowLoopback}) {
    if (!isLoopbackUrl(url) || allowLoopback) return null;
    return 'Las direcciones locales solo se permiten en compilaciones de '
        'depuración; usa la dirección de red del equipo.';
  }

  /// Why an explicit `http://` address to a private IP is refused, or null.
  /// Release builds keep cleartext to the tailnet only; debug builds allow it.
  static String? privateCleartextProblem(
    String url, {
    required bool allowLoopback,
  }) {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.scheme != 'http' || allowLoopback) return null;
    if (!_isPrivate(uri.host)) return null;
    return 'Usa la dirección de la tailnet (100.x) o https';
  }
}
