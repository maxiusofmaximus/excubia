import 'dart:async';

import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model.dart';

import '../../core/state/chat_controller.dart';
import '../agent_detail_screen.dart';
import '../theme.dart';
import '../widgets/time_label.dart';
import 'approval_entry.dart';
import 'approval_inbox.dart';
import 'chat_composer.dart';
import 'chat_conversation_view.dart';
import 'delegation_card.dart';
import 'delegation_detail_screen.dart';
import 'history_drawer.dart';

/// The chat-first home. The empty state is the product's front door; a
/// conversation is the same screen once it has a timeline.
class ChatScreen extends StatefulWidget {
  const ChatScreen({
    super.key,
    required this.controller,
    required this.onOpenTeam,
  });

  final ChatController controller;
  final VoidCallback onOpenTeam;

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final GlobalKey<ScaffoldState> _scaffold = GlobalKey<ScaffoldState>();
  final TextEditingController _composer = TextEditingController();
  final FocusNode _focus = FocusNode();
  final ScrollController _scroll = ScrollController();

  ChatController get _chat => widget.controller;

  @override
  void initState() {
    super.initState();
    _chat.addListener(_onChanged);
    _scroll.addListener(_onScroll);
  }

  @override
  void dispose() {
    _chat.removeListener(_onChanged);
    _composer.dispose();
    _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  void _onChanged() {
    if (!mounted) return;
    setState(() {});
  }

  void _onScroll() {
    if (!_scroll.hasClients || _chat.loadingMore) return;
    if (_scroll.position.pixels <= 120 && _chat.hasMore) {
      unawaited(_chat.loadOlder());
    }
  }

  void _jumpToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_scroll.hasClients) return;
      _scroll.animateTo(
        _scroll.position.maxScrollExtent,
        duration: const Duration(milliseconds: 220),
        curve: Curves.easeOut,
      );
    });
  }

  Future<void> _send() async {
    final text = _composer.text.trim();
    if (text.isEmpty || !_chat.canSend) return;
    _composer.clear();
    _jumpToBottom();
    await _chat.send(text);
    _jumpToBottom();
  }

  Future<void> _newChat() async {
    final id = await _chat.createConversation();
    if (id != null) _focus.requestFocus();
  }

  Future<void> _openDelegation(ChatEntry entry) async {
    final id = entry.delegationId;
    if (id == null || id.isEmpty) return;
    await Navigator.of(context).push<void>(
      MaterialPageRoute<void>(
        builder: (_) => DelegationDetailScreen(
          chat: _chat,
          delegationId: id,
          fallbackTitle: entry.title ?? 'Delegación',
        ),
      ),
    );
  }

  Future<void> _openAgent(ChatEntry entry) async {
    final paneId = entry.paneId;
    if (paneId == null || paneId.isEmpty) return;
    await Navigator.of(context).push<void>(
      MaterialPageRoute<void>(
        builder: (_) => AgentDetailScreen(triage: _chat.triage, paneId: paneId),
      ),
    );
  }

  Future<void> _cancelDelegation(ChatEntry entry) async {
    final id = entry.delegationId;
    if (id == null || id.isEmpty) return;
    final confirmed = await confirmDelegationCancel(
      context,
      title: entry.title ?? 'La delegación',
    );
    if (!confirmed || !mounted) return;
    final failure = await _chat.cancelDelegation(id);
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(
          failure == null
              ? 'Delegación cancelada. El historial se conserva.'
              : failure.message,
        ),
      ),
    );
  }

  Future<void> _openHistory() async {
    await _chat.loadHistory();
    if (!mounted) return;
    _scaffold.currentState?.openDrawer();
  }

  void _openApprovals() {
    final approvals = _chat.approvals;
    if (approvals == null) return;
    openApprovalSettings(
      context,
      approvals: approvals,
      conversationId: _chat.current?.id,
      projects: _chat.projects,
    );
  }

  void _suggest(String text) {
    _composer.text = text;
    _composer.selection = TextSelection.collapsed(offset: text.length);
    _focus.requestFocus();
    _jumpToBottom();
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Scaffold(
      key: _scaffold,
      backgroundColor: c.bg,
      drawer: HistoryDrawer(controller: _chat, onOpenTeam: widget.onOpenTeam),
      body: SafeArea(
        child: Column(
          children: [
            ChatHeader(
              name: _chat.alfredName,
              status: _chat.alfredStatus,
              offline: _chat.offline,
              unread: _chat.unreadMessages,
              onHistory: _openHistory,
              onNewChat: _newChat,
              approvalsPending: _chat.approvals?.userPending().length ?? 0,
              onApprovals: _chat.approvals == null ? null : _openApprovals,
            ),
            if (_chat.approvals case final approvals?)
              ApprovalInbox(controller: approvals, projects: _chat.projects),
            Expanded(
              child: _chat.loading && _chat.current == null
                  ? const Center(child: CircularProgressIndicator())
                  : _chat.current == null
                  ? _EmptyChat(
                      chat: _chat,
                      onSuggestion: _suggest,
                      onOpenTeam: widget.onOpenTeam,
                    )
                  : ConversationView(
                      chat: _chat,
                      scroll: _scroll,
                      onRetrySend: _chat.retryLocal,
                      onRetryTurn: _chat.retryTurn,
                      onRetryFailure: _chat.retryFailure,
                      onOpenDelegation: _openDelegation,
                      onOpenAgent: _openAgent,
                      onCancelDelegation: _cancelDelegation,
                    ),
            ),
            ChatComposer(
              controller: _composer,
              focus: _focus,
              enabled: _chat.canSend,
              sending: _chat.sending,
              offline: _chat.offline,
              onSend: _send,
            ),
          ],
        ),
      ),
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
    this.approvalsPending = 0,
    this.onApprovals,
  });

  final String name;
  final String status;
  final bool offline;
  final int unread;
  final VoidCallback onHistory;
  final VoidCallback onNewChat;
  final int approvalsPending;
  final VoidCallback? onApprovals;

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
          Semantics(
            label: 'Alfred',
            image: true,
            child: CircleAvatar(
              radius: 16,
              backgroundColor: c.raised,
              child: Icon(Icons.smart_toy_outlined, size: 18, color: c.accent),
            ),
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
                      child: Text(
                        offline ? 'Sin conexión' : status,
                        style: context.text.small,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
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

class _EmptyChat extends StatelessWidget {
  const _EmptyChat({
    required this.chat,
    required this.onSuggestion,
    required this.onOpenTeam,
  });

  final ChatController chat;
  final ValueChanged<String> onSuggestion;
  final VoidCallback onOpenTeam;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final recents = chat.conversations.take(3).toList();
    return ListView(
      padding: const EdgeInsets.fromLTRB(
        Spacing.lg,
        Spacing.xl,
        Spacing.lg,
        Spacing.xl,
      ),
      children: [
        Center(
          child: CircleAvatar(
            radius: 34,
            backgroundColor: c.accentSoft,
            child: Icon(Icons.smart_toy_outlined, size: 34, color: c.accent),
          ),
        ),
        const SizedBox(height: Spacing.md),
        Text(
          '¿Qué movemos hoy?',
          style: context.text.headline,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Spacing.sm),
        Text(
          'Dime un objetivo y reparto el trabajo entre Ernesto y su equipo. '
          'Yo te traigo el resultado.',
          style: context.text.bodySecondary,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Spacing.lg),
        _Suggestion(
          icon: Icons.add,
          title: 'Crea el proyecto Demo',
          detail: 'y pide a Ernesto un plan de 3 hitos',
          onTap: () => onSuggestion(
            'Crea el proyecto Demo y pide a Ernesto un plan de 3 hitos.',
          ),
        ),
        _Suggestion(
          icon: Icons.help_outline,
          title: '¿Cómo va herdr-notify?',
          detail: 'estado, bloqueos y última entrega',
          onTap: () => onSuggestion('¿Cómo va herdr-notify?'),
        ),
        _Suggestion(
          icon: Icons.check,
          title: 'Revisa los tests de la API',
          detail: 'Ines puede ejecutar la suite e2e',
          onTap: () => onSuggestion('Revisa los tests de la API.'),
        ),
        const SizedBox(height: Spacing.lg),
        _TeamStrip(chat: chat, onTap: onOpenTeam),
        if (recents.isNotEmpty) ...[
          const SizedBox(height: Spacing.lg),
          Text('RECIENTES', style: context.text.label),
          const SizedBox(height: Spacing.xs),
          for (final conversation in recents)
            _RecentRow(
              conversation: conversation,
              onTap: () => chat.openConversation(conversation.id),
            ),
        ],
      ],
    );
  }
}

class _Suggestion extends StatelessWidget {
  const _Suggestion({
    required this.icon,
    required this.title,
    required this.detail,
    required this.onTap,
  });

  final IconData icon;
  final String title;
  final String detail;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Semantics(
      button: true,
      label: '$title, $detail',
      child: Material(
        color: c.surface,
        borderRadius: BorderRadius.circular(12),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(12),
          child: Container(
            constraints: const BoxConstraints(minHeight: 56),
            margin: const EdgeInsets.only(bottom: Spacing.sm),
            padding: const EdgeInsets.all(Spacing.md),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: c.outlineSoft),
            ),
            child: Row(
              children: [
                Container(
                  width: 30,
                  height: 30,
                  decoration: BoxDecoration(
                    color: c.accentSoft,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Icon(icon, size: 18, color: c.accent),
                ),
                const SizedBox(width: Spacing.sm),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(title, style: context.text.body),
                      Text(detail, style: context.text.small),
                    ],
                  ),
                ),
                Icon(Icons.arrow_forward, size: 18, color: c.textMuted),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _TeamStrip extends StatelessWidget {
  const _TeamStrip({required this.chat, required this.onTap});

  final ChatController chat;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final agents = chat.teamAgents;
    final working = agents
        .where((agent) => agent.status.name == 'working')
        .length;
    final blocked = agents
        .where((agent) => agent.status.name == 'blocked')
        .length;
    final detail = agents.isEmpty
        ? 'Sin agentes en herdr'
        : '$working trabajando · $blocked esperan';
    return Semantics(
      button: true,
      label: 'Equipo: $detail. Abrir Equipo.',
      child: Material(
        color: c.raised,
        borderRadius: BorderRadius.circular(12),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(12),
          child: Container(
            constraints: const BoxConstraints(minHeight: 56),
            padding: const EdgeInsets.all(Spacing.md),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: c.outlineSoft),
            ),
            child: Row(
              children: [
                Icon(Icons.groups_outlined, size: 22, color: c.accent),
                const SizedBox(width: Spacing.sm),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('Equipo', style: context.text.title),
                      Text(detail, style: context.text.small),
                    ],
                  ),
                ),
                Icon(Icons.chevron_right, size: 20, color: c.textMuted),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _RecentRow extends StatelessWidget {
  const _RecentRow({required this.conversation, required this.onTap});

  final ChatConversation conversation;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Semantics(
      button: true,
      label: 'Abrir ${conversation.title}',
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: Container(
            constraints: const BoxConstraints(minHeight: 48),
            padding: const EdgeInsets.symmetric(vertical: Spacing.xs),
            child: Row(
              children: [
                Icon(Icons.chat_bubble_outline, size: 18, color: c.textMuted),
                const SizedBox(width: Spacing.sm),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        conversation.title,
                        style: context.text.body,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      Text(
                        relativeTime(conversation.lastMessageAt),
                        style: context.text.small,
                      ),
                    ],
                  ),
                ),
                if (conversation.unread > BigInt.zero)
                  Badge(label: Text('${conversation.unread}')),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
