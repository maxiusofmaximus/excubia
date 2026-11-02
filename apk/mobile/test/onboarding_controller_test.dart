import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:herdr_notify/core/api/host_client.dart';
import 'package:herdr_notify/core/state/onboarding_controller.dart';
import 'package:herdr_notify/src/rust/api/onboarding/model.dart';
import 'package:herdr_notify/src/rust/frb_generated.dart';

import 'support/fake_onboarding_host.dart';

Future<OnboardingController> loadedController(FakeOnboardingHost host) async {
  final controller = OnboardingController(transport: host);
  await controller.loadCatalog();
  return controller;
}

/// Walks the steps as the user does: picks the areas, then the level, and
/// stops on the team step with the level's preselection.
Future<OnboardingController> teamFor(
  FakeOnboardingHost host, {
  required List<String> areas,
  OnboardingLevel? level,
}) async {
  final controller = await loadedController(host);
  controller.advance();
  for (final area in areas) {
    controller.toggleArea(area);
  }
  controller.advance();
  if (level != null) controller.chooseLevel(level);
  controller.advance();
  return controller;
}

void main() {
  setUpAll(RustLib.init);

  group('catalogue', () {
    test(
      'a failed read keeps the assistant open and a retry loads it',
      () async {
        final host = standardHost()
          ..answers['GET /v1/roles'] = const HostError(0, '');
        final controller = OnboardingController(transport: host);

        await controller.loadCatalog();
        expect(controller.catalog, isNull);
        expect(controller.failure, isNotNull);

        host.answers['GET /v1/roles'] = onboardingCatalogJson;
        await controller.loadCatalog();
        expect(controller.catalog, isNotNull);
        expect(controller.failure, isNull);
        expect(controller.chosenAreas, isEmpty);
      },
    );
  });

  group('preselection', () {
    test(
      'the default level is Profesional, which picks the base team',
      () async {
        final controller = await teamFor(standardHost(), areas: ['video']);

        expect(controller.level, OnboardingLevel.profesional);
        expect(controller.step, OnboardingStep.team);
        expect(controller.hasRole('vera'), isTrue);
        expect(controller.hasRole('jade'), isTrue);
        expect(controller.hasRole('karim'), isFalse);
      },
    );

    test('Personal picks only the main role of the area', () async {
      final controller = await teamFor(
        standardHost(),
        areas: ['video'],
        level: OnboardingLevel.personal,
      );

      expect(controller.hasRole('vera'), isTrue);
      expect(controller.hasRole('jade'), isFalse);
      expect(controller.hasRole('karim'), isFalse);
    });

    test('Equipo picks every role of the area', () async {
      final controller = await teamFor(
        standardHost(),
        areas: ['video'],
        level: OnboardingLevel.equipo,
      );

      expect(controller.selectedIn(controller.chosenAreas.single.id), 3);
    });

    test('skipping the level keeps Profesional and goes to the team', () async {
      final controller = await loadedController(standardHost());
      controller.advance();
      controller.toggleArea('video');
      controller.advance();
      controller.skipLevel();

      expect(controller.step, OnboardingStep.team);
      expect(controller.level, OnboardingLevel.profesional);
      expect(controller.hasRole('jade'), isTrue);
    });

    test(
      'the preselection is recomputed when the team is entered again',
      () async {
        final controller = await teamFor(
          standardHost(),
          areas: ['video'],
          level: OnboardingLevel.personal,
        );
        controller.toggleRole(_role(controller, 'karim'));
        controller.back();
        controller.chooseLevel(OnboardingLevel.equipo);
        controller.advance();

        expect(controller.hasRole('karim'), isTrue);
        expect(controller.selectedIn(controller.chosenAreas.single.id), 3);
      },
    );

    test(
      'an area with no team proposed starts empty and cannot contract',
      () async {
        final host = standardHost();
        final controller = await teamFor(host, areas: ['otras']);

        expect(controller.step, OnboardingStep.team);
        expect(controller.canContract, isFalse);
        await controller.contract();
        expect(host.posts, isEmpty);
      },
    );
  });

  group('limit of eight per area', () {
    test('a ninth role is refused and the area is named', () async {
      final host = FakeOnboardingHost({
        'GET /v1/roles': nineRolesCatalogJson,
        'GET /v1/onboarding': onboardingPendingJson,
        'POST /v1/onboarding': onboardingSavedJson,
      });
      final controller = await teamFor(
        host,
        areas: ['video'],
        level: OnboardingLevel.equipo,
      );
      final area = controller.chosenAreas.single;
      expect(controller.selectedIn(area.id), OnboardingController.maxPerArea);

      final ninth = controller.rolesIn(area).last;
      expect(controller.toggleRole(ninth), isFalse);
      expect(controller.hasRole(ninth.id), isFalse);
      expect(controller.selectedIn(area.id), OnboardingController.maxPerArea);

      controller.toggleRole(controller.rolesIn(area).first);
      expect(controller.toggleRole(ninth), isTrue);
      expect(controller.hasRole(ninth.id), isTrue);
    });
  });

  group('contract', () {
    test('sends the exact selection once and confirms the count', () async {
      final host = standardHost();
      final controller = await teamFor(host, areas: ['video']);

      await controller.contract();

      expect(host.posts, [
        {
          'areas': ['video'],
          'level': 'profesional',
          'roles': ['vera', 'jade'],
        },
      ]);
      expect(controller.step, OnboardingStep.confirmed);
      expect(controller.contracted, 2);
      expect(controller.failure, isNull);
    });

    test('area_full keeps the selection and a retry sends it again', () async {
      final host = standardHost(
        post: HostError(
          409,
          jsonEncode({
            'detail': {'code': 'area_full'},
          }),
        ),
      );
      final controller = await teamFor(host, areas: ['video']);

      await controller.contract();
      expect(controller.failure?.message, 'Esa área ya tiene 8 agentes.');
      expect(controller.step, OnboardingStep.team);
      expect(controller.hasRole('vera'), isTrue);

      host.answers['POST /v1/onboarding'] = onboardingSavedJson;
      await controller.contract();

      expect(controller.step, OnboardingStep.confirmed);
      expect(host.posts, hasLength(2));
      expect(host.posts.last, host.posts.first);
    });
  });

  group('skip', () {
    test('marks the onboarding done and contracts nobody', () async {
      final host = standardHost();
      final controller = await loadedController(host);

      await controller.skip();

      expect(host.posts, [
        {'skip': true},
      ]);
      expect(controller.step, OnboardingStep.skipped);
      expect(controller.contracted, 0);
    });
  });

  group('onboardingPending', () {
    test('a done onboarding stays closed', () async {
      final host = standardHost(onboarding: onboardingDoneJson);

      expect(await onboardingPending(host), isFalse);
      expect(host.calls, ['GET /v1/onboarding']);
    });

    test('a pending onboarding opens when the host says can_hire, and posts nothing', () async {
      final host = standardHost(onboarding: onboardingPendingCanHireJson);

      expect(await onboardingPending(host), isTrue);
      expect(host.calls, ['GET /v1/onboarding']);
      expect(host.posts, isEmpty);
    });

    test('a device without can_hire stays closed', () async {
      final host = standardHost(onboarding: onboardingPendingNoHireJson);

      expect(await onboardingPending(host), isFalse);
    });

    test(
      'an answer without the can_hire field stays closed, as before it existed',
      () async {
        final host = standardHost(onboarding: onboardingPendingJson);

        expect(await onboardingPending(host), isFalse);
      },
    );

    test('a failed read stays closed', () async {
      final host = standardHost()
        ..answers['GET /v1/onboarding'] = const HostError(422, '');

      expect(await onboardingPending(host), isFalse);
    });

    test('a network failure stays closed', () async {
      final host = standardHost()
        ..answers['GET /v1/onboarding'] = const HostError(0, 'sin red');

      expect(await onboardingPending(host), isFalse);
    });
  });
}

CatalogRole _role(OnboardingController controller, String id) =>
    controller.catalog!.roles.firstWhere((role) => role.id == id);
