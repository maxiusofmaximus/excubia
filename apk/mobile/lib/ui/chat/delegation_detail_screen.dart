import 'dart:async';

import 'package:flutter/material.dart';
import 'package:herdr_notify/src/rust/api/chat/model_team.dart';
import 'package:herdr_notify/src/rust/api/error.dart' show FailureKind;
import 'package:herdr_notify/src/rust/api/error/chat.dart' show ChatFailure;

import '../../core/state/chat_controller.dart';
import '../agent_detail_screen.dart';
import '../theme.dart';
import '../widgets/time_label.dart';
import 'delegation_card.dart';

/// The "Ver resultado completo" destination. It fetches the whole delegation
/// once; the timeline card remains the live surface.
class DelegationDetailScreen extends StatefulWidget {
  const DelegationDetailScreen({
    super.key,
    required this.chat,
    required this.delegationId,
    required this.fallbackTitle,
  });

  final ChatController chat;
  final String delegationId;
  final String fallbackTitle;

  @override
  State<DelegationDetailScreen> createState() => _DelegationDetailScreenState();
}

class _DelegationDetailScreenState extends State<DelegationDetailScreen> {
  ChatDelegation? _detail;
  ChatFailure? _failure;
  bool _loading = true;
  bool _cancelling = false;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _failure = null;
    });
    final result = await widget.chat.delegationDetail(widget.delegationId);
    if (!mounted) return;
    setState(() {
      _detail = result.detail ?? _detail;
      _failure = result.failure;
      _loading = false;
    });
  }

  Future<void> _cancel() async {
    final detail = _detail;
    final title = detail == null
        ? widget.fallbackTitle
        : '${detail.fromName} → ${detail.toName}';
    final confirmed = await confirmDelegationCancel(context, title: title);
    if (!confirmed || !mounted) return;
    setState(() => _cancelling = true);
    final failure = await widget.chat.cancelDelegation(widget.delegationId);
    if (!mounted) return;
    setState(() => _cancelling = false);
    if (failure != null) {
      setState(() => _failure = failure);
      return;
    }
    await _load();
    if (!mounted) return;
    ScaffoldMessenger.of(context)
        .showSnackBar(const SnackBar(content: Text('Delegación cancelada.')));
  }

  Future<void> _openAgent(String paneId) async {
    await Navigator.of(context).push<void>(
      MaterialPageRoute<void>(
        builder: (_) =>
            AgentDetailScreen(triage: widget.chat.triage, paneId: paneId),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final detail = _detail;
    return Scaffold(
      backgroundColor: c.bg,
      appBar: AppBar(
        backgroundColor: c.surface,
        foregroundColor: c.textPrimary,
        title: const Text('Delegación'),
      ),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(
          Spacing.lg,
          Spacing.lg,
          Spacing.lg,
          Spacing.xl,
        ),
        children: [
          if (_loading && detail == null)
            const Padding(
              padding: EdgeInsets.all(Spacing.xl),
              child: Center(child: CircularProgressIndicator()),
            )
          else if (detail == null)
            _FailurePanel(
              failure:
                  _failure ??
                  const ChatFailure(
                    code: 'local',
                    message: 'No se pudo leer la delegación.',
                    kind: FailureKind.retry,
                    shownInTimeline: false,
                    keptByHost: false,
                  ),
              onRetry: _load,
            )
          else ...[
            _Header(detail: detail),
            if (_failure != null) ...[
              const SizedBox(height: Spacing.md),
              _FailurePanel(failure: _failure!, onRetry: _load),
            ],
            const SizedBox(height: Spacing.lg),
            _Label('TAREA'),
            const SizedBox(height: Spacing.xs),
            SelectableText(
              detail.task.trim().isEmpty
                  ? 'Sin tarea registrada.'
                  : detail.task,
              style: context.text.body,
            ),
            const SizedBox(height: Spacing.lg),
            _Label('RESULTADO'),
            const SizedBox(height: Spacing.xs),
            if (detail.resultBody != null &&
                detail.resultBody!.trim().isNotEmpty)
              SelectableText(detail.resultBody!, style: context.text.body)
            else
              Text(
                delegationIsActive(detail.state)
                    ? 'Todavía no hay resultado; la delegación sigue abierta.'
                    : 'La delegación terminó sin un resultado escrito.',
                style: context.text.bodySecondary,
              ),
            if (detail.reason != null && detail.reason!.trim().isNotEmpty) ...[
              const SizedBox(height: Spacing.md),
              _Label('MOTIVO'),
              const SizedBox(height: Spacing.xs),
              SelectableText(detail.reason!, style: context.text.bodySecondary),
            ],
            if (detail.children.isNotEmpty) ...[
              const SizedBox(height: Spacing.lg),
              _Label('DELEGACIONES DERIVADAS'),
              const SizedBox(height: Spacing.xs),
              for (final child in detail.children) _ChildRow(child: child),
            ],
            if (detail.synthesis != null && detail.synthesis!.isNotEmpty) ...[
              const SizedBox(height: Spacing.md),
              Text('Síntesis: ${detail.synthesis}', style: context.text.small),
            ],
            const SizedBox(height: Spacing.xl),
            Wrap(
              spacing: Spacing.sm,
              runSpacing: Spacing.sm,
              children: [
                if (detail.paneId != null && detail.paneId!.isNotEmpty)
                  OutlinedButton.icon(
                    onPressed: () => _openAgent(detail.paneId!),
                    style: OutlinedButton.styleFrom(
                      minimumSize: const Size(48, 48),
                    ),
                    icon: const Icon(Icons.terminal_outlined, size: 18),
                    label: const Text('Abrir agente'),
                  ),
                if (delegationIsActive(detail.state))
                  FilledButton.icon(
                    onPressed: _cancelling ? null : _cancel,
                    style: FilledButton.styleFrom(
                      minimumSize: const Size(48, 48),
                      backgroundColor: c.statusBlocked,
                      foregroundColor: c.onAccent,
                    ),
                    icon: _cancelling
                        ? const SizedBox(
                            width: 16,
                            height: 16,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Icon(Icons.stop_circle_outlined, size: 18),
                    label: const Text('Cancelar delegación'),
                  ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

class _Header extends StatelessWidget {
  const _Header({required this.detail});

  final ChatDelegation detail;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final color = delegationStatusColor(context, detail.state);
    return Container(
      padding: const EdgeInsets.all(Spacing.md),
      decoration: BoxDecoration(
        color: c.surface,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: c.outlineSoft),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.account_tree_outlined, size: 20, color: color),
              const SizedBox(width: Spacing.sm),
              Expanded(
                child: Text(
                  '${detail.fromName} pidió a ${detail.toName}',
                  style: context.text.title,
                ),
              ),
            ],
          ),
          const SizedBox(height: Spacing.sm),
          Wrap(
            spacing: Spacing.sm,
            runSpacing: Spacing.xs,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: Spacing.sm,
                  vertical: 4,
                ),
                decoration: BoxDecoration(
                  color: color.withValues(alpha: 0.12),
                  borderRadius: BorderRadius.circular(999),
                  border: Border.all(color: color.withValues(alpha: 0.35)),
                ),
                child: Text(
                  delegationStatusLabel(detail.state),
                  style: context.text.label.copyWith(color: color),
                ),
              ),
              Text('Nivel ${detail.depth}', style: context.text.small),
              if (detail.createdAt.isNotEmpty)
                Text(
                  'Creada ${relativeTime(detail.createdAt)}',
                  style: context.text.small,
                ),
              if (detail.finishedAt != null && detail.finishedAt!.isNotEmpty)
                Text(
                  'Terminada ${relativeTime(detail.finishedAt)}',
                  style: context.text.small,
                ),
            ],
          ),
        ],
      ),
    );
  }
}

class _Label extends StatelessWidget {
  const _Label(this.text);

  final String text;

  @override
  Widget build(BuildContext context) {
    return Text(text, style: context.text.label);
  }
}

class _ChildRow extends StatelessWidget {
  const _ChildRow({required this.child});

  final ChatDelegationChild child;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final color = delegationStatusColor(context, child.state);
    return Container(
      constraints: const BoxConstraints(minHeight: 48),
      margin: const EdgeInsets.only(bottom: Spacing.xs),
      padding: const EdgeInsets.symmetric(
        horizontal: Spacing.md,
        vertical: Spacing.sm,
      ),
      decoration: BoxDecoration(
        color: c.raised,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: c.outlineSoft),
      ),
      child: Row(
        children: [
          Icon(delegationStatusIcon(child.state), size: 18, color: color),
          const SizedBox(width: Spacing.sm),
          Expanded(
            child: Text(
              '${child.toName ?? 'Delegación'} · nivel ${child.depth}',
              style: context.text.body,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
          ),
          Text(
            delegationStatusLabel(child.state),
            style: context.text.small.copyWith(color: color),
          ),
        ],
      ),
    );
  }
}

class _FailurePanel extends StatelessWidget {
  const _FailurePanel({required this.failure, required this.onRetry});

  final ChatFailure failure;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Container(
      padding: const EdgeInsets.all(Spacing.md),
      decoration: BoxDecoration(
        color: c.surface,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: c.statusBlocked.withValues(alpha: 0.55)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.error_outline, size: 20, color: c.statusBlocked),
              const SizedBox(width: Spacing.sm),
              Expanded(child: Text(failure.message, style: context.text.body)),
            ],
          ),
          if (failure.action != null && failure.action!.isNotEmpty) ...[
            const SizedBox(height: Spacing.xs),
            Text(failure.action!, style: context.text.small),
          ],
          const SizedBox(height: Spacing.sm),
          OutlinedButton(
            onPressed: onRetry,
            style: OutlinedButton.styleFrom(minimumSize: const Size(48, 48)),
            child: const Text('Reintentar'),
          ),
        ],
      ),
    );
  }
}
