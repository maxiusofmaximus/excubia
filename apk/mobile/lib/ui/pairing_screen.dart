import 'dart:async';

import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api.dart' show coreVersion;
import 'package:herdr_notify/src/rust/api/error.dart';
import 'package:herdr_notify/src/rust/api/pinned.dart' as rust;
import 'package:herdr_notify/src/rust/api/policy.dart';
import 'package:herdr_notify/src/rust/api/signing.dart';

import '../core/api/pair_discovery.dart';
import '../core/api/pake_pairing.dart';
import '../core/api/pairing_address.dart';
import '../core/api/pairing_api.dart';
import '../core/api/pairing_entry.dart';
import '../core/api/pinned_client.dart';
import '../core/app_info.dart';
import '../core/storage/pairing_store.dart';
import 'qr_scan_screen.dart';
import 'theme.dart';
import 'widgets/advanced_pairing.dart';
import 'widgets/pairing_error_box.dart';
import 'widgets/pairing_sas_dialog.dart';
import 'widgets/paste_link_dialog.dart';
import 'widgets/recovery_code_dialog.dart';
import 'widgets/surface_card.dart';
import 'widgets/transport_footer.dart';

part 'pairing_screen_flows.dart';
part 'pairing_screen_pake.dart';

/// One single flow: the pairing-code card is the primary path, everything
/// manual lives collapsed under "Avanzado".
class PairingScreen extends StatefulWidget {
  const PairingScreen({
    super.key,
    required this.store,
    required this.onPaired,
    this.api,
    this.discovery,
  });

  final PairingStore store;
  final ValueChanged<Pairing> onPaired;

  /// Replaces the HTTP and pinned transports in tests.
  final PairingApi? api;

  /// Looks for the PC showing a code; tests put a fake in its place.
  final PairDiscovery? discovery;

  @override
  State<PairingScreen> createState() => _PairingScreenState();
}

class _PairingScreenState extends State<PairingScreen> {
  final TextEditingController _code = TextEditingController();
  final TextEditingController _url = TextEditingController();
  final TextEditingController _deviceId = TextEditingController(
    text: PairingStore.suggestDeviceId(),
  );
  late final PairingApi _api = widget.api ?? PairingApi();
  late final PairDiscovery _discovery =
      widget.discovery ?? const MethodChannelPairDiscovery();

  String? _privateKey;
  String? _publicKey;
  String? _fingerprint;
  String? _coreVersion;
  String? _error;
  String? _errorAction;
  String? _errorDetail;
  bool _errorSecurity = false;
  String? _status;
  rust.PairingLinkInfo? _pinnedLink;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _url.addListener(_onUrlEdited);
    unawaited(_load());
  }

  @override
  void dispose() {
    _code.dispose();
    _url.dispose();
    _deviceId.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final pairing = await widget.store.load();
    var privateKey = pairing.privateKey;
    var publicKey = privateKey.isEmpty
        ? null
        : publicKeyFromPrivate(privateKeyB64: privateKey);
    var fingerprint = privateKey.isEmpty
        ? null
        : fingerprintFromPrivate(privateKeyB64: privateKey);

    if (publicKey == null || fingerprint == null) {
      // Generate and persist as soon as a usable key exists: the operator may
      // register the shown public key on the host before the request succeeds.
      final material = generateDeviceKey();
      privateKey = material.privateKeyB64;
      publicKey = material.publicKeyB64;
      fingerprint = material.fingerprint;
      await widget.store.save(pairing.copyWith(privateKey: privateKey));
    }

    if (!mounted) return;
    setState(() {
      _privateKey = privateKey;
      _publicKey = publicKey;
      _fingerprint = fingerprint;
      _coreVersion = coreVersion();
      if (pairing.baseUrl.isNotEmpty) _url.text = pairing.baseUrl;
      if (pairing.deviceId.isNotEmpty) _deviceId.text = pairing.deviceId;
    });
  }

  /// Typing an address by hand leaves the pinned link behind: the footer must
  /// not claim a verified connection to an address the link never named.
  void _onUrlEdited() {
    if (_pinnedLink != null) setState(() => _pinnedLink = null);
  }

  Future<void> _pair({String? code, String? url}) async {
    final baseUrl = PairingAddress.normaliseUrl(url ?? _url.text);
    final pairingCode = (code ?? _code.text).trim();
    if (baseUrl.isEmpty) {
      _fail('Añade la dirección del equipo en Avanzado o escanea el QR.');
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
    final publicKey = _publicKey;
    if (privateKey == null || publicKey == null) {
      _fail('La clave del dispositivo aún no está lista.');
      return;
    }
    setState(() {
      _busy = true;
      _pinnedLink = null;
      _error = null;
      _errorAction = null;
      _errorDetail = null;
    });
    try {
      final response = await _api.pair(
        baseUrl,
        PairRequest(
          publicKey: publicKey,
          label: _deviceId.text.trim(),
          code: pairingCode,
        ),
      );
      await _finishPairing(
        Pairing(
          baseUrl: baseUrl,
          deviceId: response.deviceId,
          privateKey: privateKey,
        ),
        response.recoveryCode,
      );
    } on PairingException catch (e) {
      final info = explainPairingError(status: e.status);
      _showError(info.message, info.action);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// The CORE11 link. Its pins come with it, and the request goes out only
  /// after the host's key has matched one of them during the TLS handshake.
  Future<void> _pairPinned(rust.PairingLinkInfo link) async {
    final privateKey = _privateKey;
    final publicKey = _publicKey;
    if (privateKey == null || publicKey == null) {
      _fail('La clave del dispositivo aún no está lista.');
      return;
    }
    final pins = HostPins(primary: link.primary, backup: link.backup);
    setState(() {
      _busy = true;
      _pinnedLink = link;
      _error = null;
      _errorAction = null;
      _errorDetail = null;
    });
    try {
      final response = await _api.pairPinned(
        addresses: link.addresses,
        pins: pins,
        request: PairRequest(
          publicKey: publicKey,
          label: _deviceId.text.trim(),
          code: link.code,
        ),
      );
      await _finishPairing(
        Pairing(
          baseUrl: 'https://${response.address ?? link.addresses.first}',
          deviceId: response.deviceId,
          privateKey: privateKey,
          pins: pins,
        ),
        response.recoveryCode,
        sas: rust.pairingSas(pin: link.primary, publicKeyBase64: publicKey),
      );
    } on PairingException catch (e) {
      final kind = e.kind;
      if (kind == null) {
        final info = explainPairingError(status: e.status);
        _showError(info.message, info.action);
      } else {
        _showError(pinnedFailureMessage(kind), null, detail: e.detail);
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// A failed attempt never leaves the footer claiming a verified connection.
  void _showError(
    String message,
    String? action, {
    String? detail,
    bool security = false,
  }) {
    if (!mounted) return;
    setState(() {
      _error = message;
      _errorSecurity = security;
      _errorAction = action;
      _errorDetail = detail;
      _pinnedLink = null;
    });
  }

  void _fail(String message) => _showError(message, null);

  /// `setState` for the pairing extensions, which cannot call it directly.
  void _update(VoidCallback change) => setState(change);

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.all(Spacing.lg),
          children: [
            Text(appName, style: context.text.headline),
            const SizedBox(height: Spacing.xs),
            Text(
              appTagline,
              style: context.text.body.copyWith(
                color: context.colors.accentText,
              ),
            ),
            const SizedBox(height: Spacing.sm),
            Text(
              'Empareja este teléfono con el equipo que ejecuta Excubia.',
              style: context.text.bodySecondary,
            ),
            const SizedBox(height: Spacing.xl),
            SurfaceCard(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('Código de emparejamiento', style: context.text.title),
                  const SizedBox(height: Spacing.sm),
                  TextField(
                    controller: _code,
                    style: context.text.mono,
                    textCapitalization: TextCapitalization.characters,
                    enabled: !_busy,
                    decoration: const InputDecoration(
                      hintText: 'ABCD1234',
                      helperText: '8 caracteres',
                    ),
                  ),
                  const SizedBox(height: Spacing.sm),
                  Text(
                    'Escribe el código de 8 caracteres que muestra el PC; '
                    'la app buscará el equipo sola.',
                    style: context.text.small,
                  ),
                  const SizedBox(height: Spacing.md),
                  FilledButton(
                    onPressed: _busy ? null : _pairPressed,
                    child: Text(
                      _busy ? (_status ?? 'Emparejando…') : 'Emparejar',
                    ),
                  ),
                ],
              ),
            ),
            // Right under the card that triggered it: at the end of the list it sat off-screen.
            if (_error != null) ...[
              const SizedBox(height: Spacing.md),
              PairingErrorBox(
                message: _error!,
                action: _errorAction,
                detail: _errorDetail,
                security: _errorSecurity,
              ),
            ],
            const SizedBox(height: Spacing.md),
            OutlinedButton.icon(
              onPressed: _busy ? null : _scanQr,
              icon: const Icon(Icons.qr_code_scanner),
              label: const Text('Escanear QR'),
            ),
            const SizedBox(height: Spacing.sm),
            OutlinedButton.icon(
              onPressed: _busy ? null : _pasteLink,
              icon: const Icon(Icons.content_paste),
              label: const Text('Pegar enlace'),
            ),
            const SizedBox(height: Spacing.lg),
            AdvancedPairingSection(
              fingerprint: _fingerprint ?? 'Generando…',
              coreVersion: _coreVersion ?? '…',
              deviceIdController: _deviceId,
              urlController: _url,
            ),
            const SizedBox(height: Spacing.lg),
            TransportFooter(url: _url, pinned: _pinnedLink != null),
            const SizedBox(height: Spacing.xl),
          ],
        ),
      ),
    );
  }
}
