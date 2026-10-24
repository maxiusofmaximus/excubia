import 'dart:async';

import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/signing.dart';

import '../core/api/gateway_client.dart';
import '../core/state/approvals_controller.dart';
import '../core/state/chat_controller.dart';
import '../core/state/live_triage.dart';
import '../core/storage/pairing_store.dart';
import 'agents_screen.dart';
import 'chat/chat_screen.dart';
import 'theme.dart';

/// Two one-tap destinations: the Alfred chat (primary) and the existing team
/// triage (secondary). One signed SSE connection feeds both.
class HomeShell extends StatefulWidget {
  const HomeShell({super.key, required this.pairing, required this.onUnpaired});

  final Pairing pairing;
  final Future<void> Function() onUnpaired;

  @override
  State<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends State<HomeShell> {
  LiveTriage? _triage;
  ChatController? _chat;
  ApprovalsController? _approvals;
  bool _keyError = false;
  int _tab = 0;

  @override
  void initState() {
    super.initState();
    final publicKey = publicKeyFromPrivate(
      privateKeyB64: widget.pairing.privateKey,
    );
    if (publicKey == null) {
      _keyError = true;
      return;
    }
    final client = GatewayClient(
      baseUrl: widget.pairing.baseUrl,
      deviceId: widget.pairing.deviceId,
      privateKeyBase64: widget.pairing.privateKey,
      pins: widget.pairing.pins,
    );
    final triage = LiveTriage(client)..start();
    final approvals = ApprovalsController(
      transport: client,
      deviceId: widget.pairing.deviceId,
    );
    final chat = ChatController(client, triage, approvals: approvals);
    _triage = triage;
    _chat = chat;
    _approvals = approvals;
    chat.start();
    unawaited(approvals.refresh());
  }

  @override
  void dispose() {
    _chat?.dispose();
    _approvals?.dispose();
    _triage?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (_keyError || _triage == null || _chat == null) {
      return Scaffold(
        backgroundColor: context.colors.bg,
        body: Center(
          child: Padding(
            padding: const EdgeInsets.all(Spacing.xl),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  Icons.key_off_outlined,
                  size: 40,
                  color: context.colors.textMuted,
                ),
                const SizedBox(height: Spacing.md),
                Text(
                  'No se pudo leer la clave del dispositivo',
                  style: context.text.title,
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: Spacing.sm),
                Text(
                  'Vuelve a emparejar para generar una nueva.',
                  style: context.text.bodySecondary,
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: Spacing.lg),
                FilledButton(
                  onPressed: widget.onUnpaired,
                  child: const Text('Volver a emparejar'),
                ),
              ],
            ),
          ),
        ),
      );
    }
    final chat = _chat!;
    return Scaffold(
      backgroundColor: context.colors.bg,
      body: IndexedStack(
        index: _tab,
        children: [
          ChatScreen(
            controller: chat,
            onOpenTeam: () => setState(() => _tab = 1),
          ),
          AgentsScreen(
            triage: _triage!,
            chat: chat,
            onUnpaired: widget.onUnpaired,
          ),
        ],
      ),
      bottomNavigationBar: ListenableBuilder(
        listenable: chat,
        builder: (context, _) {
          final unread = chat.unreadMessages;
          return NavigationBar(
            selectedIndex: _tab,
            onDestinationSelected: (index) => setState(() => _tab = index),
            destinations: [
              NavigationDestination(
                icon: Badge(
                  isLabelVisible: unread > 0,
                  label: Text('$unread'),
                  child: const Icon(Icons.forum_outlined),
                ),
                selectedIcon: Badge(
                  isLabelVisible: unread > 0,
                  label: Text('$unread'),
                  child: const Icon(Icons.forum),
                ),
                label: 'Chat',
              ),
              const NavigationDestination(
                icon: Icon(Icons.groups_outlined),
                selectedIcon: Icon(Icons.groups),
                label: 'Equipo',
              ),
            ],
          );
        },
      ),
    );
  }
}
