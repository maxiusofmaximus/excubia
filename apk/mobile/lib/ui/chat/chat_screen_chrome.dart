part of 'chat_screen.dart';

/// «Ir al final»: shown only while away from the end, with the unseen count.
class _JumpToEnd extends StatelessWidget {
  const _JumpToEnd({
    required this.away,
    required this.unseen,
    required this.onTap,
  });

  final ValueListenable<bool> away;
  final ValueListenable<int> unseen;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<bool>(
      valueListenable: away,
      builder: (context, isAway, _) {
        if (!isAway) return const SizedBox.shrink();
        return ValueListenableBuilder<int>(
          valueListenable: unseen,
          builder: (context, count, _) => Semantics(
            button: true,
            label: count == 1
                ? 'Ir al final, 1 mensaje nuevo'
                : 'Ir al final, $count mensajes nuevos',
            excludeSemantics: true,
            child: FilledButton(
              onPressed: onTap,
              style: FilledButton.styleFrom(
                minimumSize: const Size(48, 48),
                padding: const EdgeInsets.symmetric(horizontal: Spacing.md),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(Icons.arrow_downward, size: 18),
                  if (count > 0) ...[
                    const SizedBox(width: Spacing.xs),
                    Text('$count'),
                  ],
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

class ChatHeader extends StatelessWidget {
  const ChatHeader({
    super.key,
    required this.name,
    required this.status,
    required this.offline,
    required this.unread,
    required this.onHistory,
    required this.onNewChat,
    required this.alertRules,
    this.connectionFailure,
    this.approvalsPending = 0,
    this.onApprovals,
    this.thinking = false,
    this.announce,
  });

  final String name;
  final String status;
  final ConnectionFailure? connectionFailure;

  /// The status as a screen reader hears it, once a new step may be spoken.
  final String? announce;
  final bool offline;
  final int unread;
  final VoidCallback onHistory;
  final VoidCallback onNewChat;
  final int approvalsPending;
  final VoidCallback? onApprovals;
  final AlertsApi alertRules;

  /// True while a turn of this conversation is queued or running; his head shows
  /// `thinking`.
  final bool thinking;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Container(
      constraints: const BoxConstraints(minHeight: 58),
      padding: const EdgeInsets.symmetric(horizontal: Spacing.sm, vertical: 4),
      decoration: BoxDecoration(
        color: c.surface,
        border: Border(bottom: BorderSide(color: c.outlineSoft)),
      ),
      child: Row(
        children: [
          IconButton(
            tooltip: 'Abrir historial',
            onPressed: onHistory,
            icon: Badge(
              isLabelVisible: unread > 0,
              label: Text('$unread'),
              child: const Icon(Icons.menu),
            ),
            color: c.textSecondary,
          ),
          AgentAvatar(
            name: name,
            size: 32,
            look: thinking ? AgentLook.thinking : null,
          ),
          const SizedBox(width: Spacing.sm),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(name, style: context.text.title),
                Row(
                  children: [
                    Container(
                      width: 7,
                      height: 7,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: offline ? c.statusBlocked : c.statusDone,
                      ),
                    ),
                    const SizedBox(width: 5),
                    Expanded(
                      child: Semantics(
                        liveRegion: announce != null,
                        label: announce,
                        child: Text(
                          offline ? connectionLabel(connectionFailure) : status,
                          style: context.text.small,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
          if (onApprovals case final onTap?)
            ApprovalsHeaderButton(pending: approvalsPending, onPressed: onTap),
          IconButton(
            tooltip: 'Avisos con la pantalla apagada',
            onPressed: () => openAlertsSettings(context, alertRules),
            icon: const Icon(Icons.notifications_outlined),
            color: c.textSecondary,
          ),
          IconButton(
            tooltip: 'Nuevo chat',
            onPressed: onNewChat,
            icon: const Icon(Icons.add_comment_outlined),
            color: c.textSecondary,
          ),
        ],
      ),
    );
  }
}
