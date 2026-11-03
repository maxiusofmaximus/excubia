part of 'pairing_screen.dart';

const _noPcFound =
    'No encontré el PC. Comprueba que el código siga visible en el PC y que '
    'los dos estén en la misma Wi-Fi, o escribe la dirección en Avanzado.';

extension _PairingScreenPake on _PairingScreenState {
  /// Eight characters, whatever the dashes or case, is the short code: the PAKE
  /// path. Seven (or a link) stays with F1.
  Future<void> _pairPressed() {
    final code = _code.text.replaceAll(RegExp(r'[\s-]'), '').toUpperCase();
    return code.length == 8 ? _pairByCode(code) : _pair();
  }

  /// The address typed in Avanzado as `host:port`, the TLS port of the PC.
  String _manualAddress() {
    final url = PairingAddress.normaliseUrl(_url.text);
    return url.isEmpty ? '' : Uri.parse(url).authority;
  }

  /// With [expectedHost] (the v2 QR) the PC is always found on the network:
  /// the address typed or saved in Avanzado is never used on this path.
  Future<void> _pairByCode(String code, {String? expectedHost}) async {
    final publicKey = _publicKey;
    if (publicKey == null) {
      _fail('La clave del dispositivo aún no está lista.');
      return;
    }
    final fromQr = expectedHost != null;
    final loopback = fromQr
        ? null
        : PairingAddress.loopbackProblem(
            PairingAddress.normaliseUrl(_url.text),
            allowLoopback: loopbackAllowed(),
          );
    if (loopback != null) {
      _fail(loopback);
      return;
    }
    final manual = fromQr ? '' : _manualAddress();
    _update(() {
      _busy = true;
      _status = manual.isEmpty ? 'Buscando el PC en la red…' : 'Emparejando…';
      _pinnedLink = null;
      _error = null;
      _errorAction = null;
      _errorDetail = null;
      _errorSecurity = false;
    });
    try {
      var addresses = [manual];
      if (manual.isEmpty) {
        try {
          addresses = await discoverWithRetry(_discovery);
        } on PairDiscoveryException catch (e) {
          _fail(e.message);
          return;
        }
        if (addresses.isEmpty) {
          _fail(_noPcFound);
          return;
        }
        if (!mounted) return;
        _update(() => _status = 'Emparejando…');
      }
      final result = await _api.pairPake(
        addresses: addresses,
        code: code,
        devicePublicKey: publicKey,
        expectedHost: expectedHost,
      );
      switch (result) {
        case PakePaired():
          await _finishPairing(
            Pairing(
              baseUrl: 'https://${result.address}',
              deviceId: result.deviceId,
              privateKey: _privateKey!,
              pins: HostPins(primary: result.pin),
            ),
            result.recoveryCode,
            sas: rust.pairingSas(pin: result.pin, publicKeyBase64: publicKey),
          );
        case PakeRefused():
          _refused(result);
      }
    } finally {
      if (mounted) {
        _update(() {
          _busy = false;
          _status = null;
        });
      }
    }
  }

  /// A code that reached the network is spent whatever came back, so the field
  /// is emptied; only a code that is not even a code stays to be corrected.
  void _refused(PakeRefused refused) {
    if (refused.kind != rust.PakeFailureKind.invalidCodeFormat) _code.clear();
    if (refused.isSecurity) {
      _showError(
        'No se pudo verificar el equipo',
        refused.message,
        security: true,
      );
      return;
    }
    final wait = refused.retryAfter;
    _fail(
      refused.kind == rust.PakeFailureKind.rateLimited && wait != null
          ? '${refused.message} Vuelve a intentarlo en $wait s.'
          : refused.message,
    );
  }
}
