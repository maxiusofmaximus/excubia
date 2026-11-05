import 'package:flutter/material.dart';

import '../../src/rust/api/approvals/model.dart';
import '../theme.dart';

/// The first twelve characters of a SHA-256: enough to compare by eye with the
/// digest the operator sees on the other side. The full value does not fit.
String shortDigest(String sha256) =>
    sha256.length <= 12 ? sha256 : '${sha256.substring(0, 12)}…';

/// The sender as the host reports it, or the account when the host sent none.
String senderLabel(String? from, String account) =>
    from == null || from.trim().isEmpty ? account : from;

String sizeLabel(int bytes) {
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1024 * 1024) {
    return '${_decimal(bytes / 1024)} KB';
  }
  return '${_decimal(bytes / (1024 * 1024))} MB';
}

String _decimal(double value) => value.toStringAsFixed(1).replaceAll('.', ',');

/// One labelled fact of an approval or a draft.
class MailFactRow extends StatelessWidget {
  const MailFactRow({super.key, required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: Spacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label, style: context.text.label),
          Text(value, style: context.text.body),
        ],
      ),
    );
  }
}

/// An attachment by its name, size and digest. The content never travels.
class MailAttachmentRow extends StatelessWidget {
  const MailAttachmentRow({
    super.key,
    required this.name,
    required this.size,
    required this.sha256,
  });

  final String name;
  final int size;
  final String sha256;

  @override
  Widget build(BuildContext context) {
    return MailFactRow(
      label: 'ADJUNTO',
      value: '$name · ${sizeLabel(size)}\nHuella ${shortDigest(sha256)}',
    );
  }
}

/// What the operator approves in a `mail.send`: the account, the addresses, the
/// subject, the start of the body and the attachments with their digests.
class MailSendFacts extends StatelessWidget {
  const MailSendFacts({super.key, required this.details});

  final MailSendDetails details;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        MailFactRow(label: 'CUENTA', value: details.account),
        MailFactRow(
          label: 'REMITENTE',
          value: senderLabel(details.from, details.account),
        ),
        MailFactRow(label: 'PARA', value: details.to.join(', ')),
        if (details.cc.isNotEmpty)
          MailFactRow(label: 'CC', value: details.cc.join(', ')),
        MailFactRow(label: 'ASUNTO', value: details.subject),
        MailFactRow(label: 'INICIO DEL CUERPO', value: details.bodyPreview),
        for (final attachment in details.attachments)
          MailAttachmentRow(
            name: attachment.name,
            size: attachment.size,
            sha256: attachment.sha256,
          ),
      ],
    );
  }
}

/// What the operator approves in a `mail.attach_path`: the file outside the
/// workspace, its size and its digest.
class MailAttachFacts extends StatelessWidget {
  const MailAttachFacts({super.key, required this.details});

  final MailAttachDetails details;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        MailFactRow(label: 'RUTA', value: details.path),
        MailFactRow(label: 'TAMAÑO', value: sizeLabel(details.size)),
        MailFactRow(label: 'Huella', value: shortDigest(details.sha256)),
      ],
    );
  }
}
