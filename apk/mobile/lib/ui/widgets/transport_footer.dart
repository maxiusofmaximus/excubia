import 'package:flutter/material.dart';

import '../../core/api/transport_note.dart';
import '../theme.dart';

/// The line under the pairing flow that names how the traffic is protected. It
/// follows the address field, so it changes as the operator types.
class TransportFooter extends StatelessWidget {
  const TransportFooter({super.key, required this.url, required this.pinned});

  final TextEditingController url;
  final bool pinned;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: url,
      builder: (context, _) {
        final note = transportNote(baseUrl: url.text.trim(), pinned: pinned);
        if (note.isEmpty) return const SizedBox.shrink();
        return Text(
          note,
          style: context.text.small,
          textAlign: TextAlign.center,
        );
      },
    );
  }
}
