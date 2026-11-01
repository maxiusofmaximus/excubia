import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/state/approval_move.dart';
import 'package:herdr_notify/src/rust/api/approvals/model.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/approval_fixtures.dart';

ChatApproval _from(Map<String, Object?> row) =>
    parseApprovalInbox(json: inboxJson([row]))!.single;

void main() {
  setUpAll(() async {
    await RustLib.init();
  });

  test('only a mail.move approval is a mail batch', () {
    expect(isMailMove(_from(moveRow())), isTrue);
    expect(isMailMove(_from(approvalRow())), isFalse);
  });

  test('the batch keeps the mails the host has not left out', () {
    final entry = _from(
      moveRow(items: [moveItem('m1'), moveItem('m2', excluded: true)]),
    );
    expect(moveItems(entry).map((item) => item.messageId), ['m1']);
    expect(checkedCount(entry, const {}), 1);
  });

  test('exclude sends the unchecked mails of the batch, sorted', () {
    final entry = _from(
      moveRow(items: [moveItem('m3'), moveItem('m1'), moveItem('m2')]),
    );
    expect(excludeFor(entry, {'m3', 'm1'}), ['m1', 'm3']);
    expect(checkedCount(entry, {'m3', 'm1'}), 1);
    expect(canApproveMove(entry, {'m3', 'm1'}), isTrue);
  });

  test('ids that are not in the batch are never sent', () {
    final entry = _from(moveRow(items: [moveItem('m1'), moveItem('m2')]));
    expect(excludeFor(entry, {'ajeno', 'm2'}), ['m2']);
    expect(excludeFor(entry, {'ajeno'}), isEmpty);
  });

  test('with every mail unchecked the batch cannot be approved', () {
    final entry = _from(moveRow(items: [moveItem('m1'), moveItem('m2')]));
    const all = {'m1', 'm2'};
    expect(canApproveMove(entry, all), isFalse);
    expect(checkedCount(entry, all), 0);
  });

  test('a batch without usable items has nothing to move or exclude', () {
    final entry = _from(approvalRow(tool: 'mail.move'));
    expect(entry.mailMove, isNull);
    expect(moveItems(entry), isEmpty);
    expect(canApproveMove(entry, const {}), isFalse);
    expect(excludeFor(entry, const {'m1'}), isEmpty);
  });
}
