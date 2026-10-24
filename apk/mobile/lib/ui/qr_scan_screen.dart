import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../core/api/pairing_entry.dart';
import 'theme.dart';

/// Camera view for the pairing QR. A readable code is handed back as a
/// [PairingEntry]; anything else the scanner saw is discarded by the parser.
class QrScanScreen extends StatefulWidget {
  const QrScanScreen({super.key});

  @override
  State<QrScanScreen> createState() => _QrScanScreenState();
}

class _QrScanScreenState extends State<QrScanScreen> {
  final MobileScannerController _controller = MobileScannerController(
    formats: const <BarcodeFormat>[BarcodeFormat.qrCode],
    detectionSpeed: DetectionSpeed.noDuplicates,
  );

  String? _error;
  bool _handled = false;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _onDetect(BarcodeCapture capture) {
    if (_handled) return;
    for (final barcode in capture.barcodes) {
      final raw = barcode.rawValue;
      if (raw == null || raw.isEmpty) continue;
      final entry = parsePairingEntry(raw);
      switch (entry) {
        case PairingEntryInvalid(:final reason):
          setState(() => _error = reason);
        case PairingEntryPinned() || PairingEntryPlain():
          _handled = true;
          Navigator.of(context).pop(entry);
          return;
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(
        title: const Text('Escanear QR'),
        leading: IconButton(
          icon: const Icon(Icons.close),
          onPressed: () => Navigator.of(context).pop(),
        ),
      ),
      body: SafeArea(
        child: Column(
          children: [
            Expanded(
              child: Stack(
                fit: StackFit.expand,
                children: [
                  MobileScanner(
                    controller: _controller,
                    onDetect: _onDetect,
                    errorBuilder: (context, error) => ColoredBox(
                      color: context.colors.bg,
                      child: Center(
                        child: Padding(
                          padding: const EdgeInsets.all(Spacing.lg),
                          child: Text(
                            'No se pudo abrir la cámara. Revisa el permiso o '
                            'escribe los datos a mano.\n\n${error.errorCode.name}',
                            style: context.text.bodySecondary,
                            textAlign: TextAlign.center,
                          ),
                        ),
                      ),
                    ),
                  ),
                  IgnorePointer(
                    child: Center(
                      child: Container(
                        width: 240,
                        height: 240,
                        decoration: BoxDecoration(
                          border: Border.all(
                            color: context.colors.accent,
                            width: 2,
                          ),
                          borderRadius: BorderRadius.circular(16),
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.all(Spacing.lg),
              child: Column(
                children: [
                  Text(
                    'Apunta al QR que muestra el gateway en el equipo.',
                    style: context.text.bodySecondary,
                    textAlign: TextAlign.center,
                  ),
                  if (_error != null) ...[
                    const SizedBox(height: Spacing.sm),
                    Text(
                      _error!,
                      style: TextStyle(color: context.colors.statusBlocked),
                      textAlign: TextAlign.center,
                    ),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
