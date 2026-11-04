import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/api/alfred_backend_api.dart';
import '../../core/api/alfred_backend_models.dart';
import '../../core/api/host_client.dart';
import '../../src/rust/api/error.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';

/// The engine switch of Alfred in the settings page: Claude or the local model on the PC.
/// Going local asks first; going back to Claude does not. While the PC is starting or
/// stopping the model, the status is read again every few seconds.
class AlfredEngineSection extends StatefulWidget {
  const AlfredEngineSection({super.key, required this.api});

  final AlfredBackendApi api;

  @override
  State<AlfredEngineSection> createState() => _AlfredEngineSectionState();
}

class _AlfredEngineSectionState extends State<AlfredEngineSection> {
  static const Duration _every = Duration(seconds: 3);

  AlfredBackendStatus? _status;
  ErrorInfo? _readFailure;
  String? _refusal;
  bool _unsupported = false;
  bool _busy = false;
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _read();
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  bool get _moving {
    final state = _status?.state;
    return state == AlfredEngineState.starting ||
        state == AlfredEngineState.stopping;
  }

  Future<void> _read() => _send(widget.api.read);

  Future<void> _change(String backend) =>
      _send(() => widget.api.change(backend));

  /// One request at a time: a tap or a poll that finds one under way does nothing.
  Future<void> _send(Future<AlfredBackendStatus> Function() request) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _refusal = null;
    });
    try {
      final status = await request();
      if (!mounted) return;
      setState(() {
        _status = status;
        _readFailure = null;
        _unsupported = false;
      });
      _syncTimer();
    } catch (error) {
      if (!mounted) return;
      _fail(error);
      // A busy PC (409) may already be changing: the screen shows what it is doing.
      if (error is HostError && error.status == 409) await _reread();
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _fail(Object error) {
    setState(() {
      if (error is HostError && error.status == 404) {
        _unsupported = true;
      } else if (_status == null) {
        _readFailure = alfredFailureOf(error);
      } else {
        _refusal = alfredFailureOf(error).message;
      }
    });
  }

  Future<void> _reread() async {
    try {
      final status = await widget.api.read();
      if (!mounted) return;
      setState(() => _status = status);
      _syncTimer();
    } catch (_) {
      // The screen keeps what it had; the refusal above already says what went wrong.
    }
  }

  void _syncTimer() {
    if (_moving) {
      _timer ??= Timer.periodic(_every, (_) {
        if (!_busy) _read();
      });
    } else {
      _timer?.cancel();
      _timer = null;
    }
  }

  Future<void> _askLocal() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: const Text('Usar el modelo local'),
        content: const Text(
          'El modelo local responde más lento y es menos preciso. Cierra el dictado '
          'por voz, usa unos 34 GB de RAM del PC y tarda unos 4 minutos en arrancar. '
          '¿Cambiar ahora?',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialog).pop(false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialog).pop(true),
            child: const Text('Cambiar'),
          ),
        ],
      ),
    );
    if (confirmed == true && mounted) await _change('local');
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('MOTOR DE ALFRED', style: context.text.label),
        const SizedBox(height: Spacing.sm),
        SurfaceCard(
          child: Opacity(
            opacity: _unsupported ? 0.5 : 1,
            child: _unsupported
                ? Text(
                    'Tu PC no admite este ajuste; actualiza Excubia en el PC.',
                    style: context.text.body,
                  )
                : _body(context),
          ),
        ),
      ],
    );
  }

  Widget _body(BuildContext context) {
    final status = _status;
    if (status == null) {
      final failure = _readFailure;
      if (failure == null) return Text('Leyendo…', style: context.text.body);
      return _column([
        Text('No se pudo leer', style: context.text.body),
        Text(failure.message, style: context.text.bodySecondary),
        _button('Reintentar', _busy ? null : _read),
      ]);
    }
    final state = status.state;
    final detail = status.detail;
    return _column([
      Text(_lineFor(status), style: context.text.body),
      if (state != AlfredEngineState.failed && detail != null)
        Text(detail, style: context.text.bodySecondary),
      if (_refusal != null)
        Text(
          _refusal!,
          style: context.text.bodySecondary.copyWith(
            color: context.colors.statusBlocked,
          ),
        ),
      const SizedBox(height: Spacing.sm),
      // The button is the next step; while the PC is moving the model it waits, disabled.
      switch (state) {
        // A failure with the turns still on the local model (Strata went down) must offer the way back.
        AlfredEngineState.claude || AlfredEngineState.failed
            when status.backend != 'local' =>
          _button('Usar el modelo local', _busy ? null : _askLocal),
        AlfredEngineState.local ||
        AlfredEngineState.failed ||
        AlfredEngineState.claude => _button(
          'Volver a Claude',
          _busy ? null : () => _change('claude'),
        ),
        AlfredEngineState.starting => _button('Usar el modelo local', null),
        AlfredEngineState.stopping => _button('Volver a Claude', null),
      },
    ]);
  }

  static String _lineFor(AlfredBackendStatus status) => switch (status.state) {
    AlfredEngineState.claude => 'Claude',
    AlfredEngineState.local => 'Modelo local',
    AlfredEngineState.starting => 'Arrancando el modelo local… (unos 4 min)',
    AlfredEngineState.stopping => 'Volviendo a Claude…',
    AlfredEngineState.failed =>
      'No pude cambiar: ${status.detail ?? 'inténtalo de nuevo.'}',
  };

  static Widget _button(String label, VoidCallback? onPressed) => Align(
    alignment: AlignmentDirectional.centerStart,
    child: OutlinedButton(onPressed: onPressed, child: Text(label)),
  );

  static Widget _column(List<Widget> children) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      for (final child in children) ...[
        child,
        const SizedBox(height: Spacing.xs),
      ],
    ],
  );
}
