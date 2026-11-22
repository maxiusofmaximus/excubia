import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/api/mail_api.dart';
import '../../core/state/mail_settings_controller.dart';
import '../theme.dart';
import '../widgets/surface_card.dart';
import 'mail_code_card.dart';
import 'mail_draft_screen.dart';

/// Settings → Correo. It connects accounts and grants or revokes sending, and
/// opens a draft by its id for review. It starts no send: a send needs the
/// operator's approval in the chat, after the review.
class MailSettingsScreen extends StatefulWidget {
  const MailSettingsScreen({super.key, required this.mail});

  final MailApi mail;

  @override
  State<MailSettingsScreen> createState() => _MailSettingsScreenState();
}

class _MailSettingsScreenState extends State<MailSettingsScreen> {
  late final MailSettingsController _controller = MailSettingsController(
    widget.mail,
  );

  @override
  void initState() {
    super.initState();
    unawaited(_controller.load());
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: context.colors.bg,
      appBar: AppBar(title: const Text('Correo')),
      body: ListenableBuilder(
        listenable: _controller,
        builder: (context, _) => ListView(
          padding: const EdgeInsets.all(Spacing.lg),
          children: _sections(context),
        ),
      ),
    );
  }

  List<Widget> _sections(BuildContext context) {
    final controller = _controller;
    final busy = controller.busy;
    final accounts = controller.accounts;
    final prompt = controller.prompt;
    return [
      Text(
        'Conecta una cuenta para que tus agentes preparen borradores. '
        'Enviar siempre necesita tu aprobación en el chat.',
        style: context.text.bodySecondary,
      ),
      const SizedBox(height: Spacing.lg),
      if (controller.loadFailure case final failure?) ...[
        SurfaceCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(failure, style: context.text.body),
              const SizedBox(height: Spacing.sm),
              TextButton(
                onPressed: busy ? null : controller.load,
                child: const Text('Reintentar'),
              ),
            ],
          ),
        ),
      ] else if (accounts == null) ...[
        if (controller.busy) const Center(child: CircularProgressIndicator()),
      ] else ...[
        Text('CUENTAS', style: context.text.label),
        const SizedBox(height: Spacing.sm),
        if (accounts.isEmpty)
          Text(
            'Todavía no hay cuentas conectadas.',
            style: context.text.bodySecondary,
          ),
        for (final account in accounts) ...[
          _AccountCard(
            account: account,
            busy: busy,
            onSendChanged: (allow) => allow
                ? controller.startSendConsent(account.label)
                : _confirmRevoke(account.label),
            onReviewDraft: () => _openDraftDialog(account.label),
          ),
          const SizedBox(height: Spacing.sm),
        ],
        const SizedBox(height: Spacing.sm),
        FilledButton.icon(
          onPressed: busy ? null : _connectDialog,
          icon: const Icon(Icons.add),
          label: const Text('Conectar una cuenta'),
        ),
      ],
      if (prompt != null) ...[
        const SizedBox(height: Spacing.lg),
        MailCodeCard(
          key: ValueKey(prompt.issuedAt),
          title: _promptTitle(prompt.purpose),
          verificationUri: prompt.start.verificationUri,
          userCode: prompt.start.userCode,
          expiresAt: prompt.expiresAt,
          busy: busy,
          onCheck: controller.checkPrompt,
        ),
      ],
      if (controller.notice case final notice?) ...[
        const SizedBox(height: Spacing.lg),
        Text(notice, style: context.text.bodySecondary),
      ],
    ];
  }

  static String _promptTitle(MailPurpose purpose) => switch (purpose) {
    MailPurpose.connect => 'Conecta tu cuenta de correo',
    MailPurpose.sendConsent => 'Permite el envío desde esta cuenta',
  };

  Future<void> _connectDialog() async {
    final choice = await showDialog<({String label, MailTenant tenant})>(
      context: context,
      builder: (_) => const _ConnectDialog(),
    );
    if (choice == null) return;
    await _controller.connect(account: choice.label, tenant: choice.tenant);
  }

  /// Turning the switch off takes the consent back, so it asks first.
  Future<void> _confirmRevoke(String account) async {
    final revoke = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Revocar el envío de «$account»'),
        content: const Text(
          'El equipo dejará de poder enviar correos desde esta cuenta. '
          'Puedes volver a permitirlo cuando quieras.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancelar'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Revocar'),
          ),
        ],
      ),
    );
    if (revoke == true) await _controller.revokeSendConsent(account);
  }

  Future<void> _openDraftDialog(String account) async {
    final draftId = await showDialog<String>(
      context: context,
      builder: (_) => const _DraftIdDialog(),
    );
    if (draftId == null || !mounted) return;
    unawaited(
      showMailDraftSheet(
        context,
        controller: _controller.draftFor(account, draftId),
      ),
    );
  }
}

class _AccountCard extends StatelessWidget {
  const _AccountCard({
    required this.account,
    required this.busy,
    required this.onSendChanged,
    required this.onReviewDraft,
  });

  final MailAccount account;
  final bool busy;

  /// Receives the new value of the switch: `true` asks for the consent,
  /// `false` asks for the confirmation before the revocation.
  final ValueChanged<bool> onSendChanged;
  final VoidCallback onReviewDraft;

  @override
  Widget build(BuildContext context) {
    final text = context.text;
    // The switch tile needs a Material ancestor; SurfaceCard only decorates.
    return SurfaceCard(
      child: Material(
        type: MaterialType.transparency,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(account.label, style: text.title),
            const SizedBox(height: Spacing.xs),
            Text(
              [?account.upn, _tenantLabel(account.tenant)].join(' · '),
              style: text.bodySecondary,
            ),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              title: const Text('Permitir enviar'),
              subtitle: Text(
                'Deja que el equipo envíe desde esta cuenta, solo cuando tú lo '
                'apruebes en el chat.',
                style: text.small,
              ),
              value: account.sendEnabled,
              onChanged: busy ? null : onSendChanged,
            ),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: TextButton.icon(
                onPressed: busy ? null : onReviewDraft,
                icon: const Icon(Icons.drafts_outlined, size: 18),
                label: const Text('Borradores por id'),
              ),
            ),
          ],
        ),
      ),
    );
  }

  static String _tenantLabel(String tenant) => switch (tenant) {
    'consumers' => 'Cuenta personal',
    'organizations' => 'Cuenta de trabajo o escuela',
    _ => 'Cuenta de correo',
  };
}

/// Asks for the id of a draft the team created. The host has no listing, so the
/// id is the only way in. An empty id keeps the dialog open.
class _DraftIdDialog extends StatefulWidget {
  const _DraftIdDialog();

  @override
  State<_DraftIdDialog> createState() => _DraftIdDialogState();
}

class _DraftIdDialogState extends State<_DraftIdDialog> {
  final TextEditingController _id = TextEditingController();

  @override
  void dispose() {
    _id.dispose();
    super.dispose();
  }

  void _submit() {
    final id = _id.text.trim();
    if (id.isEmpty) return;
    Navigator.of(context).pop(id);
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Revisar un borrador'),
      content: TextField(
        controller: _id,
        autofocus: true,
        decoration: const InputDecoration(
          labelText: 'Id del borrador',
          helperText: 'El equipo lo muestra al crear el borrador.',
        ),
        onSubmitted: (_) => _submit(),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancelar'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Abrir')),
      ],
    );
  }
}

/// Asks for a name and the kind of account. An empty name keeps the dialog open.
class _ConnectDialog extends StatefulWidget {
  const _ConnectDialog();

  @override
  State<_ConnectDialog> createState() => _ConnectDialogState();
}

class _ConnectDialogState extends State<_ConnectDialog> {
  final TextEditingController _label = TextEditingController();
  MailTenant _tenant = MailTenant.consumers;

  @override
  void dispose() {
    _label.dispose();
    super.dispose();
  }

  void _submit() {
    final label = _label.text.trim();
    if (label.isEmpty) return;
    Navigator.of(context).pop((label: label, tenant: _tenant));
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Conectar una cuenta'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          TextField(
            controller: _label,
            autofocus: true,
            decoration: const InputDecoration(
              labelText: 'Nombre de la cuenta',
              hintText: 'trabajo, personal…',
            ),
            onSubmitted: (_) => _submit(),
          ),
          const SizedBox(height: Spacing.lg),
          SegmentedButton<MailTenant>(
            segments: const [
              ButtonSegment(
                value: MailTenant.consumers,
                label: Text('Personal'),
              ),
              ButtonSegment(
                value: MailTenant.organizations,
                label: Text('Trabajo o escuela'),
              ),
            ],
            selected: {_tenant},
            onSelectionChanged: (choice) =>
                setState(() => _tenant = choice.single),
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancelar'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Conectar')),
      ],
    );
  }
}
