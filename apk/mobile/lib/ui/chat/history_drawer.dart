import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';
import 'package:herdr_notify/src/rust/api/error/chat.dart';

import '../../core/api/phone_contacts.dart';
import '../../core/state/chat_controller.dart';
import '../settings_screen.dart';
import '../theme.dart';
import '../widgets/time_label.dart';

part 'history_drawer_actions.dart';
part 'history_drawer_filters.dart';
part 'history_drawer_list.dart';
part 'history_text_prompt.dart';

/// `wf41`: the drawer keeps the chat on screen while the history, its filters
/// and the per-row actions are one tap away.
class HistoryDrawer extends StatefulWidget {
  const HistoryDrawer({
    super.key,
    required this.controller,
    required this.onOpenTeam,
    required this.onUnpaired,
  });

  final ChatController controller;
  final VoidCallback onOpenTeam;
  final Future<void> Function() onUnpaired;

  @override
  State<HistoryDrawer> createState() => _HistoryDrawerState();
}

class _HistoryDrawerState extends State<HistoryDrawer> {
  late final TextEditingController _search = TextEditingController(
    text: widget.controller.query,
  );

  ChatController get _chat => widget.controller;

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Drawer(
      width: 330,
      backgroundColor: c.surface,
      // Square edge: the Material default rounds the corners on the open side.
      shape: const RoundedRectangleBorder(),
      child: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _header(context),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Spacing.md),
              child: TextField(
                controller: _search,
                onChanged: _chat.setQuery,
                textInputAction: TextInputAction.search,
                style: context.text.body,
                decoration: const InputDecoration(
                  hintText: 'Buscar en títulos y mensajes…',
                  prefixIcon: Icon(Icons.search),
                  isDense: true,
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.all(Spacing.md),
              child: FilledButton.icon(
                onPressed: _newChat,
                icon: const Icon(Icons.add),
                label: const Text('Nuevo chat con Alfred'),
              ),
            ),
            _filters(context),
            const Divider(height: 1),
            Expanded(child: _list(context)),
            const Divider(height: 1),
            Wrap(
              alignment: WrapAlignment.spaceBetween,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                TextButton(
                  onPressed: () {
                    _chat.setFilter(HistoryFilter.archived);
                    Navigator.of(context).maybePop();
                  },
                  child: Text('Ver archivados (${_chat.archivedCount})'),
                ),
                TextButton(
                  onPressed: () {
                    final navigator = Navigator.of(context);
                    navigator.maybePop();
                    navigator.push(
                      MaterialPageRoute<void>(
                        builder: (_) => SettingsScreen(
                          mail: widget.controller.mail,
                          voice: widget.controller.voiceSend,
                          agenda: widget.controller.agenda,
                          phone: const ChannelPhoneContacts(),
                          alertRules: widget.controller.alertRules,
                          alfred: widget.controller.alfredBackend,
                          catalog: widget.controller.triage.loadCatalog,
                          setDefaultAgent:
                              widget.controller.triage.setDefaultAgent,
                          onUnpaired: widget.onUnpaired,
                          approvals: widget.controller.approvals,
                          conversationId: widget.controller.current?.id,
                          projects: widget.controller.projects,
                          alfredSeat: widget.controller.alfredSeat,
                        ),
                      ),
                    );
                  },
                  child: const Text('Ajustes'),
                ),
                TextButton(
                  onPressed: () {
                    Navigator.of(context).maybePop();
                    widget.onOpenTeam();
                  },
                  child: const Text('Equipo →'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _header(BuildContext context) {
    final c = context.colors;
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.md,
        Spacing.sm,
        Spacing.sm,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Historial', style: context.text.headline),
                Text(
                  '${_chat.pinnedCount} fijados · '
                  '${_chat.conversations.length} '
                  '${_chat.conversations.length == 1 ? 'visible' : 'visibles'} · '
                  '${_chat.archivedCount} archivados',
                  style: context.text.small,
                ),
              ],
            ),
          ),
          IconButton(
            tooltip: 'Cerrar historial',
            onPressed: () => Navigator.of(context).maybePop(),
            icon: const Icon(Icons.close),
            color: c.textSecondary,
          ),
        ],
      ),
    );
  }
}
