/// The text a pairing QR carries, once it has been read and trusted to be ours.
///
/// Only two facts survive parsing: where the gateway is and the short code that
/// registers this phone with it. The string comes from a camera, so it is
/// untrusted input and every check below is a pinned case in the tests.
class PairingQrPayload {
  const PairingQrPayload({required this.baseUrl, required this.code});

  final String baseUrl;
  final String code;

  static const String scheme = 'herdr-notify';
  static const String host = 'pair';
  static const String version = '1';
  static const int maxLength = 2048;

  static PairingQrResult parse(String raw) {
    final text = raw.trim();
    if (text.isEmpty) return _invalid('El código QR está vacío.');
    if (text.length > maxLength) {
      return _invalid('El código QR es demasiado grande para procesarlo.');
    }

    final uri = Uri.tryParse(text);
    if (uri == null) return _invalid('El código QR no se pudo leer.');

    if (uri.scheme.toLowerCase() != scheme) {
      return _invalid('Este código QR no es de Excubia.');
    }
    if ((uri.host).toLowerCase() != host) {
      return _invalid('Este código QR no es de emparejamiento.');
    }

    final params = _queryParams(uri.query);

    final versionValue = params['v'];
    if (versionValue != null && versionValue != version) {
      return _invalid(
        'Este código QR es de una versión más nueva; actualiza la app.',
      );
    }

    final url = (params['u'] ?? '').trim();
    final code = (params['c'] ?? '').trim();
    if (url.isEmpty) {
      return _invalid('Al código QR le falta la dirección del equipo.');
    }
    if (code.isEmpty) {
      return _invalid('Al código QR le falta el código de emparejamiento.');
    }

    // A scanned URL is not guessed a scheme for: the gateway always emits a
    // full URL, so a bare host means the payload is not the one we expect.
    if (!url.startsWith('http://') && !url.startsWith('https://')) {
      return _invalid(
        'La dirección del QR no empieza por http:// ni https://.',
      );
    }

    return PairingQrResult.valid(PairingQrPayload(baseUrl: url, code: code));
  }

  static PairingQrResult _invalid(String reason) =>
      PairingQrResult.invalid(reason);

  /// First value winning, tolerant of a missing `=`: a duplicate key cannot
  /// append a second `u=` or `c=` to override the real one.
  static Map<String, String> _queryParams(String? rawQuery) {
    if (rawQuery == null || rawQuery.isEmpty) return const {};
    final result = <String, String>{};
    for (final pair in rawQuery.split('&')) {
      if (pair.isEmpty) continue;
      final eq = pair.indexOf('=');
      final key = eq < 0 ? pair : pair.substring(0, eq);
      final value = eq < 0 ? '' : pair.substring(eq + 1);
      final decodedKey = _decode(key);
      if (result.containsKey(decodedKey)) continue;
      result[decodedKey] = _decode(value);
    }
    return result;
  }

  /// Best-effort percent-decoding; a malformed escape is left as-is.
  static String _decode(String value) {
    try {
      return Uri.decodeQueryComponent(value);
    } on ArgumentError {
      return value;
    } on FormatException {
      return value;
    }
  }
}

sealed class PairingQrResult {
  const PairingQrResult();

  factory PairingQrResult.valid(PairingQrPayload payload) = PairingQrValid;
  factory PairingQrResult.invalid(String reason) = PairingQrInvalid;
}

class PairingQrValid extends PairingQrResult {
  const PairingQrValid(this.payload);

  final PairingQrPayload payload;
}

class PairingQrInvalid extends PairingQrResult {
  const PairingQrInvalid(this.reason);

  final String reason;
}
