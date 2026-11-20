import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/host_client.dart';
import 'package:excubia/core/api/plans_api.dart';
import 'package:excubia/core/api/recipes_api.dart';
import 'package:excubia/core/api/restore_api.dart';
import 'package:excubia/core/api/schedules_api.dart';

void main() {
  // What a device without the grant reads: plain words, never the permission code.
  test(
    'no-grant texts of restore, plans, recipes and schedules hide the codes',
    () {
      final texts = [
        restoreFailureText(const HostError(403, '{}')),
        planDecisionFailureText(const HostError(403, '{}')),
        recipeRunFailureText(const HostError(403, '{}')),
        scheduleActionFailureText(const HostError(403, '{}')),
      ];
      for (final text in texts) {
        expect(text, isNot(contains('APPROVE')), reason: text);
        expect(text, isNot(contains('READ_STATE')), reason: text);
      }
      expect(restoreGrantLede, contains('permiso'));
      expect(planPermissionText, contains('permiso'));
      expect(recipePermissionText, contains('permiso'));
      expect(schedulePermissionText, contains('permiso'));
    },
  );
}
