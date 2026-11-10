import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/agenda_api.dart';
import 'package:excubia/core/api/alerts_api.dart';
import 'package:excubia/core/api/alerts_models.dart';
import 'package:excubia/core/api/alerts_failure_text.dart';
import 'package:excubia/core/api/host_client.dart';

import '../support/recording_transport.dart';

const String _paneRule =
    '{"id":"ar_1","trigger":"pane_status","agent":"Bob","status":"done",'
    '"action":"notify","enabled":true,"created_by":"user",'
    '"created_at":"2026-10-29T09:00:00.000Z"}';

const String _calendarRule =
    '{"id":"ar_2","trigger":"calendar_reminder","agent":null,"status":null,'
    '"action":"mute","enabled":false,"created_by":"alfred",'
    '"created_at":"2026-10-29T09:00:00.000Z"}';

void main() {
  late RecordingTransport transport;
  late AlertsApi api;

  setUp(() {
    transport = RecordingTransport();
    api = AlertsApi(transport);
  });

  group('reglas', () {
    test('lee la lista de reglas', () async {
      transport.answer = '{"ok":true,"rules":[$_paneRule,$_calendarRule]}';

      final rules = await api.alertsRules();

      expect(transport.targets.single, 'GET /v1/alerts/rules');
      expect(rules.map((rule) => rule.id), ['ar_1', 'ar_2']);
      expect(rules.last.action, AlertAction.mute);
      expect(rules.last.enabled, isFalse);
    });

    test('crear una regla de agente manda agente, estado y acción', () async {
      transport.answer = '{"ok":true,"rule":$_paneRule}';

      final rule = await api.createAlertRule(
        trigger: AlertTrigger.paneStatus,
        agent: 'Bob',
        status: AlertRuleStatus.done,
        action: AlertAction.notify,
      );

      expect(transport.targets.single, 'POST /v1/alerts/rules');
      expect(transport.bodies.single, {
        'trigger': 'pane_status',
        'agent': 'Bob',
        'status': 'done',
        'action': 'notify',
      });
      expect(rule.id, 'ar_1');
    });

    test('«Cualquiera» deja el agente fuera del cuerpo', () async {
      transport.answer = '{"ok":true,"rule":$_paneRule}';

      await api.createAlertRule(
        trigger: AlertTrigger.paneStatus,
        status: AlertRuleStatus.blocked,
        action: AlertAction.mute,
      );

      expect(transport.bodies.single, {
        'trigger': 'pane_status',
        'status': 'blocked',
        'action': 'mute',
      });
    });

    test('un recordatorio no manda agente ni estado', () async {
      transport.answer = '{"ok":true,"rule":$_calendarRule}';

      await api.createAlertRule(
        trigger: AlertTrigger.calendarReminder,
        action: AlertAction.mute,
      );

      expect(transport.bodies.single, {
        'trigger': 'calendar_reminder',
        'action': 'mute',
      });
    });

    test('activar y desactivar manda enabled al id de la regla', () async {
      transport.answer = '{"ok":true,"rule":$_paneRule}';

      final rule = await api.setAlertRuleEnabled('ar_1', false);

      expect(transport.targets.single, 'POST /v1/alerts/rules/ar_1/update');
      expect(transport.bodies.single, {'enabled': false});
      expect(rule.id, 'ar_1');
    });

    test('borrar va al id de la regla', () async {
      transport.answer = '{"ok":true,"id":"ar_1"}';

      await api.deleteAlertRule('ar_1');

      expect(transport.targets.single, 'POST /v1/alerts/rules/ar_1/delete');
    });
  });

  group('modo', () {
    test('lee el modo que el host tiene', () async {
      transport.answer = '{"ok":true,"mode":"rules_only"}';

      expect(await api.alertsMode(), AlertsMode.rulesOnly);
      expect(transport.targets.single, 'GET /v1/alerts/mode');
    });

    test('cambiar el modo manda el modo y devuelve el que quedó', () async {
      transport.answer = '{"ok":true,"mode":"all"}';

      expect(await api.setAlertsMode(AlertsMode.all), AlertsMode.all);
      expect(transport.targets.single, 'POST /v1/alerts/mode');
      expect(transport.bodies.single, {'mode': 'all'});
    });
  });

  group('agentes', () {
    test('los nombres vienen del estado, sin repetirse y en orden', () async {
      transport.answer = '{"revision":3,"agents":[{"agent":"Bob"},{"agent":"Ana"},{"agent":"Bob"}]}';

      expect(await api.agentNames(), ['Ana', 'Bob']);
      expect(transport.targets.single, 'GET /v1/state');
    });
  });

  group('errores', () {
    String failure(int status, String body) =>
        alertsFailureText(HostError(status, body));

    test('una regla igual se dice en español', () {
      expect(
        failure(409, '{"detail":"rule_exists"}'),
        'Ya existe una regla igual.',
      );
    });

    test('el código también se lee dentro de detail.code', () {
      expect(
        failure(409, '{"detail":{"code":"rule_exists"}}'),
        'Ya existe una regla igual.',
      );
    });

    test('el tope de reglas se dice con su número', () {
      expect(
        failure(409, '{"detail":"rule_limit"}'),
        'Has llegado al máximo de 100 reglas.',
      );
    });

    test('sin el permiso sale el texto estándar de la agenda', () {
      expect(failure(403, ''), agendaFailureText(const HostError(403, '')));
    });

    test('una regla que ya no existe usa el texto de la agenda', () {
      expect(
        failure(404, '{"detail":{"code":"not_found"}}'),
        agendaFailureText(const HostError(404, '')),
      );
    });

    test('un error que no es de la API no se inventa un texto', () {
      expect(
        failure(500, 'no es json'),
        agendaFailureText(const HostError(500, 'no es json')),
      );
    });
  });
}
