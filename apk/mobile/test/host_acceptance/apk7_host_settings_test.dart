import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/core/api/pairing_api.dart';
import 'package:herdr_notify/core/api/plan_runner.dart';
import 'package:herdr_notify/core/state/approvals_controller.dart';
import 'package:herdr_notify/src/rust/api/chat/plan.dart';
import 'package:herdr_notify/src/rust/api/signing.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

/// Approval settings against a real host, from a freshly paired device. It runs
/// only when the environment names the host, so the normal suite skips it:
///   APK7_HOST_URL    e.g. http://127.0.0.1:18878
///   APK7_PAIR_CODE   a single-use code minted with `host pair-code`
///   APK7_HOST_DATA   the host's data dir (for `devices grant`)
///   APK7_FAKE_HOST   the host binary, used only to grant APPROVE
/// The device starts without APPROVE: the 403 must name the canonical command.
final String? _host = Platform.environment['APK7_HOST_URL'];

String _env(String name) => Platform.environment[name] ?? '';

void _log(String line) => stdout.writeln('APK7: $line');

void main() {
  setUpAll(() async {
    await RustLib.init();
    // flutter_test replaces HttpClient with a stub; the acceptance needs the real one.
    HttpOverrides.global = null;
  });

  test(
    'real host: settings without APPROVE show the command, then global and project modes',
    () async {
      final base = _host!;
      final key = generateDeviceKey();
      final pair = await PairingApi().pair(
        base,
        PairRequest(
          publicKey: key.publicKeyB64,
          label: 'APK7 settings',
          code: _env('APK7_PAIR_CODE'),
        ),
      );
      final deviceId = pair.deviceId;
      _log('paired device $deviceId without APPROVE');

      final transport = GatewayClient(
        baseUrl: base,
        deviceId: deviceId,
        privateKeyBase64: key.privateKeyB64,
      );
      final approvals = ApprovalsController(
        transport: transport,
        deviceId: deviceId,
      );

      // 1. No APPROVE: the read works, the write is refused by the host (403)
      //    and the controls turn off with the canonical command.
      await approvals.loadSettings();
      expect(approvals.settingsFailure, isNull, reason: 'the read needs no APPROVE');
      expect(await approvals.setMode(mode: 'always_user'), isFalse);
      expect(approvals.settingsDenied, isTrue);
      expect(
        approvals.settingsFailure?.command,
        'host devices grant $deviceId --permission approve',
      );
      _log('write 403 -> ${approvals.settingsFailure?.command}; controls off');

      // 2. Grant APPROVE on the host, then the same controller can read and write.
      final grant = await Process.run(_env('APK7_FAKE_HOST'), [
        'devices',
        'grant',
        deviceId,
        '--permission',
        'approve',
        '--data-dir',
        _env('APK7_HOST_DATA'),
      ]);
      expect(grant.exitCode, 0, reason: grant.stderr.toString());
      await approvals.loadSettings();
      expect(approvals.settingsDenied, isFalse);
      final before = approvals.settings?.global;
      _log('APPROVE granted; global is $before');

      // 3. Global switch: always_user, then back to delegated.
      expect(await approvals.setMode(mode: 'always_user'), isTrue);
      await approvals.loadSettings();
      expect(approvals.settings?.global, 'always_user');
      _log('global -> always_user (read back)');
      expect(await approvals.setMode(mode: 'delegated'), isTrue);
      await approvals.loadSettings();
      expect(approvals.settings?.global, 'delegated');
      _log('global -> delegated (read back)');

      // 4. Per-project override. The project must exist in the bootstrap.
      final bootstrap = jsonDecode(
        await runPlan(transport, planChatBootstrap()),
      );
      final projects = (bootstrap['projects'] as List?) ?? const [];
      _log('bootstrap projects: ${projects.length}');
      if (projects.isEmpty) {
        _log('NV per-project override: no project on this host (no team created)');
      } else {
        final projectId = projects.first['id'] as String;
        expect(
          await approvals.setMode(mode: 'always_user', projectId: projectId),
          isTrue,
        );
        await approvals.loadSettings();
        final override = approvals.settings?.projects.firstWhere(
          (project) => project.projectId == projectId,
        );
        expect(override?.mode, 'always_user');
        _log('project $projectId -> always_user (read back)');
      }

      // 5. An unknown project is refused by the host and the error is kept.
      expect(
        await approvals.setMode(mode: 'always_user', projectId: 'prj_unknown'),
        isFalse,
      );
      _log('unknown project -> ${approvals.settingsFailure?.code ?? 'refused'}');

      approvals.dispose();
    },
    skip: _host == null ? 'set APK7_HOST_URL to run against the real host' : false,
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
