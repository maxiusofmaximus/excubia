import 'package:herdr_notify/src/rust/api/approvals/model.dart';

/// Whether the approval moves mail: its batch has one checkbox per mail.
bool isMailMove(ChatApproval entry) => entry.tool == 'mail.move';

/// The batch's mails that the host has not already left out.
List<MailMoveItem> moveItems(ChatApproval entry) => [
  for (final item in entry.mailMove?.items ?? const <MailMoveItem>[])
    if (!item.excluded) item,
];

/// How many mails of the batch stay checked; the button reads «K de N».
int checkedCount(ChatApproval entry, Set<String> unchecked) =>
    moveItems(entry)
        .where((item) => !unchecked.contains(item.messageId))
        .length;

/// A move needs one mail still checked: with none, «Denegar» is the answer.
bool canApproveMove(ChatApproval entry, Set<String> unchecked) =>
    checkedCount(entry, unchecked) > 0;

/// The `exclude` of a decision: the unchecked mails that belong to this batch.
/// Sorted, so a retry with the same mails compares equal.
List<String> excludeFor(ChatApproval entry, Set<String> unchecked) {
  final batch = {for (final item in moveItems(entry)) item.messageId};
  return [
    for (final id in unchecked)
      if (batch.contains(id)) id,
  ]..sort();
}
