import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/agenda_api.dart';
import 'package:herdr_notify/core/api/alerts_api.dart';
import 'package:herdr_notify/core/api/gateway_client.dart';
import 'package:herdr_notify/ui/alerts/alert_rules_screen.dart';

import '../support/scripted_transport.dart';
import '../support/widget_mount.dart';

const Map<String, Object?> _paneRule = {
  'id': 'ar_1',
  'trigger': 'pane_status',
  'agent': 'Bob',
  'status': 'done',
  'action': 'notify',
  'enabled': true,
};

const Map<String, Object?> _newPaneRule = {
  'id': 'ar_9',
  'trigger': 'pane_status',
  'agent': 'Bob',
  'status': 'done',
  'action': 'notify',
  'enabled': true,
};

const Map<String, Object?> _calendarRule = {
  'id': 'ar_2',
  'trigger': 'calendar_reminder',
  'agent': null,
  'status': null,
  'action': 'mute',
  'enabled': true,
};

String _rules(List<Map<String, Object?>> rules) => okJson({'rules': rules});

/// The routes every screen test needs: the mode is «Avisar de todo» unless a test says so.
Map<String, String> _routes({List<Map<String, Object?>> rules = const []}) => {
  'GET /v1/alerts/rules': _rules(rules),
  'GET /v1/alerts/mode': okJson({'mode': 'all'}),
  'POST /v1/alerts/mode': okJson({'mode': 'rules_only'}),
  'GET /v1/state': jsonEncode({
    'agents': [
      {'agent': 'Bob'},
      {'agent': 'Ana'},
    ],
  }),
};

void main() {
  group('pantalla de reglas', () {
    testWidgets('el modo se lee y cambiarlo se manda al host', (tester) async {
      final transport = ScriptedTransport(_routes());

      await mountWidget(tester, AlertRulesScreen(api: AlertsApi(transport)));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Solo lo que yo elija'));
      await tester.pumpAndSettle();

      expect(transport.bodies['POST /v1/alerts/mode'], [
        {'mode': 'rules_only'},
      ]);
      await unmountWidget(tester);
    });

    testWidgets('un fallo al cambiar el modo se dice', (tester) async {
      final transport = ScriptedTransport(_routes());
      transport.failNext('POST /v1/alerts/mode', const GatewayError(403, ''));

      await mountWidget(tester, AlertRulesScreen(api: AlertsApi(transport)));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Solo lo que yo elija'));
      await tester.pumpAndSettle();

      expect(
        find.text(agendaFailureText(const GatewayError(403, ''))),
        findsOneWidget,
      );
      await unmountWidget(tester);
    });

    testWidgets('sin reglas se explica cómo crear una', (tester) async {
      final transport = ScriptedTransport(_routes());

      await mountWidget(tester, AlertRulesScreen(api: AlertsApi(transport)));
      await tester.pumpAndSettle();

      expect(find.textContaining('Aún no hay reglas'), findsOneWidget);
      await unmountWidget(tester);
    });

    testWidgets(
      'cada regla se dice en palabras y su interruptor la activa o desactiva',
      (tester) async {
        final transport = ScriptedTransport(_routes(rules: [_paneRule]))
          ..routes['POST /v1/alerts/rules/ar_1/update'] = okJson({
            'rule': {..._paneRule, 'enabled': false},
          });

        await mountWidget(tester, AlertRulesScreen(api: AlertsApi(transport)));
        await tester.pumpAndSettle();
        expect(find.text('Avisar cuando Bob termine'), findsOneWidget);

        await tester.tap(find.byType(Switch));
        await tester.pumpAndSettle();

        expect(transport.bodies['POST /v1/alerts/rules/ar_1/update'], [
          {'enabled': false},
        ]);
        expect(tester.widget<Switch>(find.byType(Switch)).value, isFalse);
        await unmountWidget(tester);
      },
    );

    testWidgets('un fallo al activar se dice y el interruptor no cambia', (
      tester,
    ) async {
      final transport = ScriptedTransport(_routes(rules: [_paneRule]));
      transport.failNext(
        'POST /v1/alerts/rules/ar_1/update',
        const GatewayError(404, '{"detail":{"code":"not_found"}}'),
      );

      await mountWidget(tester, AlertRulesScreen(api: AlertsApi(transport)));
      await tester.pumpAndSettle();
      await tester.tap(find.byType(Switch));
      await tester.pumpAndSettle();

      expect(find.text('Ya no existe. Vuelve a la lista.'), findsOneWidget);
      expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
      await unmountWidget(tester);
    });

    testWidgets('borrar pide confirmación: Cancelar no borra y Borrar sí', (
      tester,
    ) async {
      final transport = ScriptedTransport(_routes(rules: [_paneRule]))
        ..routes['POST /v1/alerts/rules/ar_1/delete'] = okJson({'id': 'ar_1'});

      await mountWidget(tester, AlertRulesScreen(api: AlertsApi(transport)));
      await tester.pumpAndSettle();

      await tester.tap(find.byTooltip('Borrar regla'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cancelar'));
      await tester.pumpAndSettle();
      expect(
        transport.calls,
        isNot(contains('POST /v1/alerts/rules/ar_1/delete')),
      );
      expect(find.text('Avisar cuando Bob termine'), findsOneWidget);

      await tester.tap(find.byTooltip('Borrar regla'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Borrar'));
      await tester.pumpAndSettle();
      expect(transport.calls, contains('POST /v1/alerts/rules/ar_1/delete'));
      expect(find.text('Avisar cuando Bob termine'), findsNothing);
      await unmountWidget(tester);
    });
  });

  group('nueva regla', () {
    testWidgets(
      'una regla de agente se crea con su agente, su cuándo y su acción',
      (tester) async {
        final transport = ScriptedTransport(_routes())
          ..routes['POST /v1/alerts/rules'] = okJson({'rule': _newPaneRule});

        await mountWidget(tester, AlertRulesScreen(api: AlertsApi(transport)));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Nueva regla'));
        await tester.pumpAndSettle();

        await tester.tap(find.text('Termina'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Cualquiera'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Bob'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Crear regla'));
        await tester.pumpAndSettle();

        expect(transport.bodies['POST /v1/alerts/rules'], [
          {
            'trigger': 'pane_status',
            'agent': 'Bob',
            'status': 'done',
            'action': 'notify',
          },
        ]);
        expect(find.text('Avisar cuando Bob termine'), findsOneWidget);
        await unmountWidget(tester);
      },
    );

    testWidgets('un recordatorio no pide agente ni cuándo', (tester) async {
      final transport = ScriptedTransport(_routes())
        ..routes['POST /v1/alerts/rules'] = okJson({'rule': _calendarRule});

      await mountWidget(tester, AlertRulesScreen(api: AlertsApi(transport)));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Nueva regla'));
      await tester.pumpAndSettle();

      await tester.tap(find.text('Recordatorio'));
      await tester.pumpAndSettle();
      expect(find.text('Nombre del agente'), findsNothing);
      expect(find.text('Termina'), findsNothing);

      await tester.tap(find.text('Silenciar'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Crear regla'));
      await tester.pumpAndSettle();

      expect(transport.bodies['POST /v1/alerts/rules'], [
        {'trigger': 'calendar_reminder', 'action': 'mute'},
      ]);
      expect(
        find.text('Silenciar los recordatorios del calendario'),
        findsOneWidget,
      );
      await unmountWidget(tester);
    });

    testWidgets('una regla igual se dice y el formulario sigue abierto', (
      tester,
    ) async {
      final transport = ScriptedTransport(_routes());
      transport.failNext(
        'POST /v1/alerts/rules',
        const GatewayError(409, '{"detail":"rule_exists"}'),
      );

      await mountWidget(tester, AlertRulesScreen(api: AlertsApi(transport)));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Nueva regla'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Crear regla'));
      await tester.pumpAndSettle();

      expect(find.text('Ya existe una regla igual.'), findsOneWidget);
      expect(find.text('Crear regla'), findsOneWidget);
      await unmountWidget(tester);
    });
  });
}
