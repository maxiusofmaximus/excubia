import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/error.dart';

import '../theme.dart';
import 'error_box.dart';
import 'surface_card.dart';

/// Respond, allowlisted keys and the destructive stop, in one card. The
/// parent owns the state; this widget only renders and forwards taps.
class AgentComposer extends StatelessWidget {
  const AgentComposer({
    super.key,
    required this.controller,
    required this.ready,
    required this.sending,
    required this.stopping,
    required this.canStop,
    required this.hint,
    required this.onRespond,
    required this.onKey,
    required this.onStop,
    this.result,
    this.resultOk = true,
    this.error,
  });

  final TextEditingController controller;
  final bool ready;
  final bool sending;
  final bool stopping;
  final bool canStop;
  final String hint;
  final VoidCallback onRespond;
  final void Function(String key) onKey;
  final VoidCallback onStop;
  final String? result;
  final bool resultOk;
  final ErrorInfo? error;

  static const List<({String label, String key, String? tooltip})> _keys = [
    (label: 'Aprobar (y)', key: 'y', tooltip: null),
    (label: 'Rechazar (n)', key: 'n', tooltip: null),
    (label: 'Enter', key: 'enter', tooltip: null),
    (label: 'Esc', key: 'esc', tooltip: null),
    (label: '↑', key: 'up', tooltip: 'Arriba'),
    (label: '↓', key: 'down', tooltip: 'Abajo'),
    (label: '←', key: 'left', tooltip: 'Izquierda'),
    (label: '→', key: 'right', tooltip: 'Derecha'),
  ];

  @override
  Widget build(BuildContext context) {
    return SurfaceCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Responder', style: context.text.title),
          const SizedBox(height: Spacing.xs),
          Text(hint, style: context.text.small),
          const SizedBox(height: Spacing.md),
          TextField(
            controller: controller,
            enabled: ready,
            minLines: 2,
            maxLines: 3,
            style: context.text.body,
            decoration: const InputDecoration(
              hintText: 'Escribe la respuesta para el agente…',
            ),
          ),
          const SizedBox(height: Spacing.md),
          FilledButton(
            onPressed: ready ? onRespond : null,
            child: sending
                ? const Row(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(
                          strokeWidth: 2,
                          color: Colors.white,
                        ),
                      ),
                      SizedBox(width: Spacing.sm),
                      Text('Enviando…'),
                    ],
                  )
                : const Text('Enviar'),
          ),
          const SizedBox(height: Spacing.md),
          Wrap(
            spacing: Spacing.sm,
            runSpacing: Spacing.sm,
            children: [
              for (final entry in _keys)
                _KeyButton(
                  label: entry.label,
                  enabled: ready,
                  tooltip: entry.tooltip,
                  onPressed: () => onKey(entry.key),
                ),
            ],
          ),
          if (error != null) ...[
            const SizedBox(height: Spacing.md),
            ErrorBox(error: error!),
          ] else if (result != null) ...[
            const SizedBox(height: Spacing.md),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  resultOk ? Icons.check_circle_outline : Icons.error_outline,
                  size: 18,
                  color: resultOk
                      ? context.colors.statusDone
                      : context.colors.statusBlocked,
                ),
                const SizedBox(width: Spacing.sm),
                Expanded(
                  child: Text(
                    result!,
                    style: context.text.small.copyWith(
                      color: resultOk
                          ? context.colors.statusDone
                          : context.colors.statusBlocked,
                    ),
                  ),
                ),
              ],
            ),
          ],
          if (canStop) ...[
            Padding(
              padding: EdgeInsets.symmetric(vertical: Spacing.md),
              child: Divider(height: 1, color: context.colors.outlineSoft),
            ),
            OutlinedButton.icon(
              onPressed: stopping || sending ? null : onStop,
              icon: stopping
                  ? SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(
                        strokeWidth: 2,
                        color: context.colors.statusBlocked,
                      ),
                    )
                  : Icon(
                      Icons.stop_circle_outlined,
                      color: context.colors.statusBlocked,
                    ),
              label: Text(stopping ? 'Deteniendo…' : 'Detener agente'),
              style: OutlinedButton.styleFrom(
                foregroundColor: context.colors.statusBlocked,
                side: BorderSide(
                  color: context.colors.statusBlocked.withValues(alpha: 0.6),
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _KeyButton extends StatelessWidget {
  const _KeyButton({
    required this.label,
    required this.enabled,
    required this.onPressed,
    this.tooltip,
  });

  final String label;
  final bool enabled;
  final VoidCallback onPressed;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final button = OutlinedButton(
      onPressed: enabled ? onPressed : null,
      style: OutlinedButton.styleFrom(
        minimumSize: const Size(0, 48),
        padding: const EdgeInsets.symmetric(horizontal: Spacing.md),
      ),
      child: Text(label),
    );
    return tooltip == null ? button : Tooltip(message: tooltip!, child: button);
  }
}
