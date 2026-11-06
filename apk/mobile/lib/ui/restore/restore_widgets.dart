import 'package:flutter/material.dart';

import '../../core/api/restore_models.dart';
import '../theme.dart';
import 'restore_text.dart';

/// Every button of the screen is at least 48 dp high, the touch target the app keeps.
const ButtonStyle restoreTouch = ButtonStyle(
  minimumSize: WidgetStatePropertyAll(Size(88, 48)),
);

/// One copy in the list: its label and clock, how long ago, and how many files.
class RestorePointTile extends StatelessWidget {
  const RestorePointTile({
    super.key,
    required this.point,
    required this.now,
    required this.onTap,
  });

  final RestorePoint point;
  final int now;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => ListTile(
    minTileHeight: 56,
    title: Text(
      '${pointLabel(point)} · ${pointTime(point.createdMs)}',
      style: context.text.body,
    ),
    subtitle: Text(
      '${pointAge(point.createdMs, now)} · ${filesText(point.files)}',
      style: context.text.small,
    ),
    trailing: Icon(Icons.chevron_right, color: context.colors.textMuted),
    onTap: onTap,
  );
}

/// One file the copy would change, with the word for the change beside it.
class RestoreChangeTile extends StatelessWidget {
  const RestoreChangeTile({super.key, required this.change});

  final RestoreChange change;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Spacing.sm),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Text(
              change.path,
              style: context.text.mono,
              softWrap: true,
            ),
          ),
          const SizedBox(width: Spacing.sm),
          DecoratedBox(
            decoration: BoxDecoration(
              border: Border.all(color: colors.outlineSoft),
              borderRadius: BorderRadius.circular(6),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: Spacing.sm,
                vertical: 2,
              ),
              child: Text(
                changeLabel(change.status),
                style: context.text.small,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// A line of words in a box: a notice from the host or the grant a device needs.
class RestoreNotice extends StatelessWidget {
  const RestoreNotice({super.key, required this.text, this.alert = false});

  final String text;

  /// An alert is announced as it appears, so a refused restore is not missed.
  final bool alert;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final body = Container(
      width: double.infinity,
      padding: const EdgeInsets.all(Spacing.md),
      decoration: BoxDecoration(
        color: colors.raised,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(
          color: alert ? colors.statusBlocked : colors.outlineSoft,
        ),
      ),
      child: Text(
        text,
        style: context.text.body,
      ),
    );
    return Semantics(liveRegion: alert, child: body);
  }
}

/// The buttons of one step, wrapped so large text never pushes one off the screen.
class RestoreActions extends StatelessWidget {
  const RestoreActions({super.key, required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: Spacing.lg),
    child: Wrap(
      alignment: WrapAlignment.end,
      spacing: Spacing.sm,
      runSpacing: Spacing.sm,
      children: children,
    ),
  );
}
