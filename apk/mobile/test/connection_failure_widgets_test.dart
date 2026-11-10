import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/alerts_api.dart';
import 'package:excubia/core/api/connection_failure.dart';
import 'package:excubia/core/api/connection_text.dart';
import 'package:excubia/core/api/plans_api.dart';
import 'package:excubia/core/api/recipes_api.dart';
import 'package:excubia/core/api/schedules_api.dart';
import 'package:excubia/core/api/team_summary_api.dart';
import 'package:excubia/core/state/chat_controller.dart';
import 'package:excubia/core/state/hires_controller.dart';
import 'package:excubia/core/state/live_triage.dart';
import 'package:excubia/core/state/plans_controller.dart';
import 'package:excubia/core/state/recipes_controller.dart';
import 'package:excubia/core/state/schedules_controller.dart';
import 'package:excubia/core/state/team_summary_controller.dart';
import 'package:excubia/src/rust/frb_generated.dart';
import 'package:excubia/ui/agents_screen.dart';
import 'package:excubia/ui/chat/chat_screen.dart';
import 'package:excubia/ui/theme.dart';

import 'support/connection_fixtures.dart';
import 'support/scripted_transport.dart';

void main() {
  setUpAll(RustLib.init);

  testWidgets('the chat header names why the PC cannot be reached', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: Scaffold(
          body: ChatHeader(
            name: 'Alfred',
            status: 'Disponible',
            offline: true,
            connectionFailure: ConnectionFailure.hostDown,
            unread: 0,
            onHistory: () {},
            onNewChat: () {},
            alertRules: AlertsApi(ScriptedTransport({})),
          ),
        ),
      ),
    );

    expect(find.text('PC sin servicio'), findsOneWidget);
    expect(find.text('Sin conexión'), findsNothing);
  });

  testWidgets('the team screen names the cause in its subtitle and banner', (
    tester,
  ) async {
    late LiveTriage triage;
    late ChatController chat;
    late HiresController hires;
    final client = refusingClient();
    triage = LiveTriage(client);
    chat = ChatController(client, triage);
    hires = HiresController(transport: client);
    await tester.runAsync(() async {
      triage.start();
      await settled(triage);
      triage.stop();
    });
    addTearDown(triage.dispose);
    addTearDown(chat.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(),
        home: AgentsScreen(
          triage: triage,
          chat: chat,
          hires: hires,
          recipes: RecipesController(
            api: RecipesApi(ScriptedTransport(const {})),
            conversationId: () => null,
          ),
          summary: TeamSummaryController(
            api: TeamSummaryApi(ScriptedTransport(const {})),
          ),
          plans: PlansController(api: PlansApi(ScriptedTransport(const {}))),
          schedules: SchedulesController(
            api: SchedulesApi(ScriptedTransport(const {})),
          ),
          onUnpaired: () async {},
        ),
      ),
    );

    expect(
      find.textContaining('Sin ruta al PC · último estado'),
      findsOneWidget,
    );
    // The banner sits below the plans, summary and hires of a lazy list.
    await tester.dragUntilVisible(
      find.textContaining(connectionText(ConnectionFailure.noRoute)),
      find.byType(ListView),
      const Offset(0, -120),
    );
    expect(
      find.textContaining(connectionText(ConnectionFailure.noRoute)),
      findsOneWidget,
    );
  });
}
