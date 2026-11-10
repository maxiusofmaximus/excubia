import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/hires_controller.dart';
import 'package:excubia/src/rust/api/onboarding/model.dart';
import 'package:excubia/src/rust/frb_generated.dart';

import 'support/fake_team_host.dart';

void main() {
  setUpAll(RustLib.init);

  group('reading the team', () {
    test('hiresByArea groups the people by area, in the catalogue order', () async {
      final host = FakeTeamHost(
        hires: [
          bobHire(),
          bobHire(id: 'hire_2', name: 'Vera')
            ..['role'] = 'vera'
            ..['area'] = 'video',
          bobHire(id: 'hire_3', name: 'Ines')..['area'] = 'area_fuera_catalogo',
        ],
      );
      final hires = HiresController(transport: host);
      await hires.load();

      final groups = hires.hiresByArea();

      expect(groups.map((group) => group.label), [
        'Vídeo y postproducción',
        'Programación',
        'Equipo',
      ]);
      expect(groups[1].hires.single.name, 'Bob');
    });

    test('load reads the catalogue once, then only the hires', () async {
      final host = FakeTeamHost(hires: [bobHire()]);
      final hires = HiresController(transport: host);

      expect(await hires.load(), isTrue);
      expect(await hires.load(), isTrue);

      expect(hires.hires.map((hire) => hire.name), ['Bob']);
      expect(hires.canHire, isTrue);
      expect(hires.catalog!.areas.map((area) => area.id), [
        'video',
        'programacion',
      ]);
      expect(
        host.calls.where((route) => route == 'GET /v1/roles'),
        hasLength(1),
      );
    });

    test(
      'can_hire false is read as false: the team is shown, not changed',
      () async {
        final hires = HiresController(transport: FakeTeamHost(canHire: false));

        await hires.load();

        expect(hires.canHire, isFalse);
        expect(hires.hires, isEmpty);
      },
    );

    test(
      'a network failure is kept for the screen and nothing is retried',
      () async {
        final host = FakeTeamHost()..networkDown = true;
        final hires = HiresController(transport: host);

        expect(await hires.load(), isFalse);

        expect(hires.failure, isNotNull);
        expect(hires.failure!.message, isNotEmpty);
        expect(hires.busy, isFalse);
        expect(host.calls, ['GET /v1/roles']);
      },
    );

    test('clearFailure removes the message only', () async {
      final host = FakeTeamHost()..networkDown = true;
      final hires = HiresController(transport: host);
      await hires.load();

      hires.clearFailure();

      expect(hires.failure, isNull);
    });
  });

  group('contracting', () {
    test('hire sends the role alone, returns the new hire and reads the list again', () async {
      final host = FakeTeamHost();
      final hires = HiresController(transport: host);
      await hires.load();

      final hire = await hires.hire(_vera(hires));

      expect(hire?.name, 'Vera');
      expect(host.posts.last, {'role': 'vera'});
      expect(hires.isHired('vera'), isTrue);
      expect(host.calls.last, 'GET /v1/hires');
    });

    test('a role already hired is not sent again', () async {
      final host = FakeTeamHost();
      final hires = HiresController(transport: host);
      await hires.load();
      await hires.hire(_vera(hires));

      final again = await hires.hire(_vera(hires));

      expect(again, isNull);
      expect(host.posts, hasLength(1));
    });

    test(
      'area_full keeps the list, says so, and sends the request once',
      () async {
        final host = FakeTeamHost()..areaFull = true;
        final hires = HiresController(transport: host);
        await hires.load();

        final hire = await hires.hire(_vera(hires));

        expect(hire, isNull);
        expect(hires.failure!.message, 'Esa área ya tiene 8 agentes.');
        expect(hires.hires, isEmpty);
        expect(host.posts, hasLength(1));
      },
    );
  });

  group('changing a hire', () {
    test('a rename sends only the name, and the character stays', () async {
      final host = FakeTeamHost(hires: [bobHire()]);
      final hires = HiresController(transport: host);
      await hires.load();

      final ok = await hires.rename(hires.hires.first, 'Roberto');

      expect(ok, isTrue);
      expect(host.posts.last, {'name': 'Roberto'});
      expect(hires.hires.first.name, 'Roberto');
      expect(
        hires.hires.first.avatar,
        'bob',
        reason: 'renaming never moves the face',
      );
    });

    test('a name the host would refuse is not sent', () async {
      final host = FakeTeamHost(hires: [bobHire()]);
      final hires = HiresController(transport: host);
      await hires.load();

      for (final name in ['', '   ', 'a' * 41, 'Ana\u0007']) {
        expect(
          await hires.rename(hires.hires.first, name),
          isFalse,
          reason: name,
        );
        expect(
          hires.failure!.message,
          'El nombre debe tener entre 1 y 40 caracteres, sin caracteres de control.',
        );
      }
      expect(host.posts, isEmpty);
    });

    test('a character is sent by its id and read back', () async {
      final host = FakeTeamHost(hires: [bobHire()]);
      final hires = HiresController(transport: host);
      await hires.load();

      final ok = await hires.changeAvatar(hires.hires.first, 'ines');

      expect(ok, isTrue);
      expect(host.posts.last, {'avatar': 'ines'});
      expect(hires.hires.first.avatar, 'ines');
      expect(hires.hires.first.name, 'Bob');
    });

    test(
      'the initial is sent as null; a host that keeps the character says so',
      () async {
        final host = FakeTeamHost(hires: [bobHire()]);
        final hires = HiresController(transport: host);
        await hires.load();

        final ok = await hires.changeAvatar(hires.hires.first, null);

        expect(ok, isFalse);
        expect(host.posts.last, {'avatar': null});
        expect(
          hires.failure!.message,
          'El equipo no ha guardado el personaje.',
        );
        expect(hires.hires.first.avatar, 'bob');
      },
    );

    test('a character outside the eight is refused before sending', () async {
      final host = FakeTeamHost(hires: [bobHire()]);
      final hires = HiresController(transport: host);
      await hires.load();

      expect(await hires.changeAvatar(hires.hires.first, 'alfred'), isFalse);

      expect(hires.failure!.message, 'Ese personaje no está disponible.');
      expect(host.posts, isEmpty);
    });

    test(
      'dismiss takes the person off the list; dismissing again changes nothing',
      () async {
        final host = FakeTeamHost(hires: [bobHire()]);
        final hires = HiresController(transport: host);
        await hires.load();

        final hire = hires.hires.first;
        expect(await hires.dismiss(hire), isTrue);
        expect(hires.hires, isEmpty);
        expect(await hires.dismiss(hire), isTrue);

        expect(hires.hires, isEmpty);
        expect(host.posts, hasLength(2));
        expect(host.posts.every((post) => post.isEmpty), isTrue);
      },
    );

    test(
      'a dismissed hire that is changed says it is no longer active',
      () async {
        final host = FakeTeamHost(hires: [bobHire()]);
        final hires = HiresController(transport: host);
        await hires.load();
        final hire = hires.hires.first;
        await hires.dismiss(hire);

        expect(await hires.rename(hire, 'Roberto'), isFalse);
        expect(hires.failure!.message, 'Ese agente ya fue despedido.');
      },
    );
  });
}

/// The catalogue entry of Vera, read from the catalogue the controller holds.
CatalogRole _vera(HiresController hires) => hires.roleById('vera')!;
