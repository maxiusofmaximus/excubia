import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/api/chat_transport.dart';
import '../../core/api/gateway_client.dart';
import '../../core/state/onboarding_controller.dart';
import '../../core/storage/pairing_store.dart';
import 'onboarding_screen.dart';

/// Opens the first-run assistant over the app when this device still owes its
/// team. The app is shown while the check runs, and it stays the app whenever
/// the check does not say the assistant is due.
class OnboardingGate extends StatefulWidget {
  const OnboardingGate({
    super.key,
    required this.pairing,
    required this.home,
    this.transport,
  });

  final Pairing pairing;
  final Widget home;

  /// Replaces the signed client; tests inject a fake host here.
  final ChatTransport? transport;

  @override
  State<OnboardingGate> createState() => _OnboardingGateState();
}

class _OnboardingGateState extends State<OnboardingGate> {
  late final ChatTransport _transport =
      widget.transport ?? GatewayClient.forPairing(widget.pairing);
  late final OnboardingController _controller = OnboardingController(
    transport: _transport,
  );
  bool _pending = false;

  @override
  void initState() {
    super.initState();
    unawaited(_check());
  }

  Future<void> _check() async {
    final pending = await onboardingPending(_transport);
    if (mounted && pending) setState(() => _pending = true);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!_pending) return widget.home;
    return OnboardingScreen(
      controller: _controller,
      onDone: () => setState(() => _pending = false),
    );
  }
}
