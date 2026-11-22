import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/api/plan_models.dart';
import 'package:excubia/ui/plans/plan_text.dart';

Map<String, Object?> _plan({
  String id = 'plan_1',
  String status = 'proposed',
  List<Map<String, Object?>> steps = const [
    {
      'title': 'Leer el repo',
      'touches': ['files'],
    },
  ],
  String summary = 'Plan de 1 paso · lo propone Ernesto · toca: ficheros',
  String? note,
  String? updatedAt,
}) => {
  'id': id,
  'status': status,
  'steps': steps,
  'summary': summary,
  'note': ?note,
  'created_at': '2026-11-05T10:00:00.000Z',
  'updated_at': ?updatedAt,
};

TeamPlan _parsed(Map<String, Object?> json) => TeamPlan.fromJson(json);

void main() {
  group('the list of plans', () {
    test('parses every plan the host sends, in the host order', () {
      final plans = teamPlansFrom({
        'ok': true,
        'plans': [
          _plan(id: 'plan_a'),
          _plan(id: 'plan_b', status: 'approved'),
        ],
      });

      expect(plans.map((plan) => plan.id), ['plan_a', 'plan_b']);
      expect(plans.last.status, PlanStatus.approved);
      expect(plans.first.steps.single.title, 'Leer el repo');
      expect(plans.first.steps.single.touches, [PlanTouch.files]);
    });

    test('a status the host does not name is a broken answer', () {
      expect(
        () => _parsed(_plan(status: 'quizá')),
        throwsA(isA<FormatException>()),
      );
    });

    test('a touch the app does not know reads as «other», and is kept', () {
      final plan = _parsed(
        _plan(
          steps: [
            {
              'title': 'Algo nuevo',
              'touches': ['galaxia'],
            },
          ],
        ),
      );

      expect(plan.steps.single.touches, [PlanTouch.other]);
    });

    test('a plan with no note or times still parses', () {
      final plan = _parsed(_plan());

      expect(plan.note, isNull);
      expect(plan.updatedAt, isNull);
      expect(plan.isProposed, isTrue);
    });

    test('the answer of one decision carries its plan', () {
      final plan = teamPlanFrom({
        'ok': true,
        'plan': _plan(id: 'plan_9', status: 'rejected'),
      });

      expect(plan.id, 'plan_9');
      expect(plan.status, PlanStatus.rejected);
    });

    test('a list that is not a list is broken, not an empty team', () {
      expect(
        () => teamPlansFrom({'ok': true, 'plans': 'no'}),
        throwsA(isA<FormatException>()),
      );
    });
  });

  group('the words of a plan', () {
    test('each touch has its plain label', () {
      expect(planTouchLabel(PlanTouch.files), 'Toca ficheros');
      expect(planTouchLabel(PlanTouch.network), 'Usa internet');
      expect(planTouchLabel(PlanTouch.mail), 'Toca correo');
      expect(planTouchLabel(PlanTouch.tools), 'Usa herramientas');
    });

    test('the summary names the touches once each, in a fixed order', () {
      const steps = [
        PlanStep(title: 'a', touches: [PlanTouch.network]),
        PlanStep(title: 'b', touches: [PlanTouch.files, PlanTouch.network]),
        PlanStep(title: 'c', touches: []),
        PlanStep(title: 'd', touches: []),
      ];

      expect(planStepsLine(steps), '4 pasos · toca ficheros e internet');
    });

    test('a plan that touches nothing reads as read-only', () {
      expect(
        planStepsLine(const [PlanStep(title: 'a', touches: [])]),
        '1 paso · solo lee',
      );
    });

    test('three touches join with commas and «y»', () {
      const steps = [
        PlanStep(
          title: 'a',
          touches: [PlanTouch.files, PlanTouch.mail, PlanTouch.tools],
        ),
      ];

      expect(
        planStepsLine(steps),
        '1 paso · toca ficheros, correo y herramientas',
      );
    });

    test('only a mail or internet step asks for the extra confirmation', () {
      TeamPlan withTouches(List<PlanTouch> touches) => _parsed(
        _plan(
          steps: [
            {
              'title': 'x',
              'touches': touches.map((touch) => touch.name).toList(),
            },
          ],
        ),
      );

      expect(needsExtraConfirm(withTouches([PlanTouch.mail])), isTrue);
      expect(needsExtraConfirm(withTouches([PlanTouch.network])), isTrue);
      expect(needsExtraConfirm(withTouches([PlanTouch.files])), isFalse);
      expect(needsExtraConfirm(withTouches([PlanTouch.tools])), isFalse);
      expect(needsExtraConfirm(withTouches([])), isFalse);
    });

    test('the status words are the ones the desktop uses', () {
      expect(planStatusLabel(PlanStatus.proposed), 'Espera tu visto bueno');
      expect(planStatusLabel(PlanStatus.approved), 'Aprobado');
      expect(planStatusLabel(PlanStatus.rejected), 'Rechazado');
      expect(planStatusLabel(PlanStatus.changesRequested), 'Pediste cambios');
    });

    test('a note must say something and stay within 300 characters', () {
      expect(planNoteProblem('   '), 'Escribe qué cambiarías del plan.');
      expect(planNoteProblem('Cambia el paso 2'), isNull);
      expect(planNoteProblem('a' * planNoteMax), isNull);
      expect(
        planNoteProblem('a' * (planNoteMax + 1)),
        'La nota admite 300 caracteres como máximo.',
      );
    });
  });

  group('the order of the list', () {
    test('proposals come first in the host order, then decided ones newest first', () {
      final sorted = sortPlans([
        _parsed(_plan(id: 'old', status: 'approved', updatedAt: '2026-11-04T09:00:00.000Z')),
        _parsed(_plan(id: 'first')),
        _parsed(_plan(id: 'recent', status: 'rejected', updatedAt: '2026-11-05T09:00:00.000Z')),
        _parsed(_plan(id: 'second')),
      ]);

      expect(sorted.map((plan) => plan.id), ['first', 'second', 'recent', 'old']);
    });

    test('sorting leaves the list it was given as it was', () {
      final given = [
        _parsed(_plan(id: 'approved', status: 'approved')),
        _parsed(_plan(id: 'proposed')),
      ];

      sortPlans(given);

      expect(given.map((plan) => plan.id), ['approved', 'proposed']);
    });
  });
}
