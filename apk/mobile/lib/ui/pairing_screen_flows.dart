part of 'pairing_screen.dart';

extension _PairingScreenFlows on _PairingScreenState {
  /// The one place a scanned or pasted string is acted on, whatever its source.
  Future<void> _applyEntry(PairingEntry entry) async {
    switch (entry) {
      case PairingEntryPinned(:final link):
        await _pairPinned(link);
      case PairingEntryPake(:final code, :final hostFingerprint):
        await _pairByCode(code, expectedHost: hostFingerprint);
      case PairingEntryPlain(:final payload):
        _url.text = payload.baseUrl;
        _code.text = payload.code;
        await _pair(code: payload.code, url: payload.baseUrl);
      case PairingEntryInvalid(:final reason):
        _fail(reason);
    }
  }

  Future<void> _finishPairing(
    Pairing pairing,
    String? recovery, {
    String? sas,
  }) async {
    await widget.store.save(pairing);
    if (recovery != null && recovery.isNotEmpty) {
      await widget.store.saveRecoveryCode(recovery);
    }
    if (!mounted) return;
    if (sas != null) {
      await showPairingSasDialog(context, sas);
      if (!mounted) return;
    }
    if (recovery != null && recovery.isNotEmpty) {
      await showRecoveryCodeDialog(context, recovery);
    }
    widget.onPaired(pairing);
  }

  Future<void> _saveAndContinue() async {
    final baseUrl = PairingAddress.normaliseUrl(_url.text);
    if (baseUrl.isEmpty) {
      _fail('La dirección del PC está vacía.');
      return;
    }
    final loopback = PairingAddress.loopbackProblem(
      baseUrl,
      allowLoopback: loopbackAllowed(),
    );
    if (loopback != null) {
      _fail(loopback);
      return;
    }
    final cleartext = PairingAddress.privateCleartextProblem(
      baseUrl,
      allowLoopback: loopbackAllowed(),
    );
    if (cleartext != null) {
      _fail(cleartext);
      return;
    }
    final privateKey = _privateKey;
    if (privateKey == null) {
      _fail('La clave del dispositivo aún no está lista.');
      return;
    }
    final pairing = Pairing(
      baseUrl: baseUrl,
      deviceId: _deviceId.text.trim(),
      privateKey: privateKey,
    );
    await widget.store.save(pairing);
    if (mounted) widget.onPaired(pairing);
  }

  Future<void> _scanQr() async {
    final entry = await Navigator.of(context).push<PairingEntry>(
      MaterialPageRoute<PairingEntry>(builder: (_) => const QrScanScreen()),
    );
    if (entry == null || !mounted) return;
    await _applyEntry(entry);
  }

  Future<void> _pasteLink() async {
    final text = await showPasteLinkDialog(context);
    if (text == null || !mounted) return;
    await _applyEntry(parsePairingEntry(text));
  }
}
