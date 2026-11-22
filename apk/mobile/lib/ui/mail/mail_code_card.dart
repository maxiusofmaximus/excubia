import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme.dart';
import '../widgets/surface_card.dart';

/// The code the operator types on the Microsoft page. It shows the code and the
/// address, copies either one, and counts down to [expiresAt]. The chat uses it
/// without [onCheck]: the host removes that card when the sign-in ends. It only
/// ticks while mounted.
class MailCodeCard extends StatefulWidget {
  const MailCodeCard({
    super.key,
    required this.title,
    required this.verificationUri,
    required this.userCode,
    required this.expiresAt,
    this.busy = false,
    this.onCheck,
  });

  final String title;
  final String verificationUri;
  final String userCode;

  /// Null when the host sent no usable time: the card then shows no countdown.
  final DateTime? expiresAt;
  final bool busy;

  /// Asks the host once whether the code was used. Null hides the button.
  final VoidCallback? onCheck;

  @override
  State<MailCodeCard> createState() => _MailCodeCardState();
}

class _MailCodeCardState extends State<MailCodeCard> {
  Timer? _ticker;

  @override
  void initState() {
    super.initState();
    _ticker = Timer.periodic(
      const Duration(seconds: 1),
      (_) => setState(() {}),
    );
  }

  @override
  void dispose() {
    _ticker?.cancel();
    super.dispose();
  }

  void _copy(String text) {
    unawaited(Clipboard.setData(ClipboardData(text: text)));
  }

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    final expiresAt = widget.expiresAt;
    final left = expiresAt?.difference(DateTime.now());
    return SurfaceCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(widget.title, style: text.title),
          const SizedBox(height: Spacing.md),
          Text('1. Abre esta página en el navegador', style: text.body),
          Text(widget.verificationUri, style: text.bodySecondary),
          TextButton.icon(
            onPressed: () => _copy(widget.verificationUri),
            icon: const Icon(Icons.copy_outlined, size: 18),
            label: const Text('Copiar enlace'),
          ),
          const SizedBox(height: Spacing.sm),
          Text('2. Introduce este código', style: text.body),
          Text(widget.userCode, style: text.headline),
          TextButton.icon(
            onPressed: () => _copy(widget.userCode),
            icon: const Icon(Icons.copy_outlined, size: 18),
            label: const Text('Copiar código'),
          ),
          if (left != null) ...[
            const SizedBox(height: Spacing.xs),
            Text(
              left <= Duration.zero
                  ? 'El código ha caducado. Empieza de nuevo.'
                  : 'Caduca en ${_countdown(left)}',
              style: text.small,
            ),
          ],
          if (widget.onCheck case final check?) ...[
            const SizedBox(height: Spacing.md),
            FilledButton(
              onPressed: widget.busy ? null : check,
              child: const Text('Ya lo he hecho, comprobar'),
            ),
          ],
        ],
      ),
    );
  }

  static String _countdown(Duration left) =>
      '${left.inMinutes}:${(left.inSeconds % 60).toString().padLeft(2, '0')}';
}
