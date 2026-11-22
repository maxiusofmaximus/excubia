import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/ui/widgets/agent_avatar.dart';

void main() {
  test('an id finds its character whatever the visible name says', () {
    expect(agentProfileById('bob')?.id, 'bob');
    expect(agentProfileById('alfred')?.id, 'alfred');
    expect(agentProfileById('nadie'), isNull);
    expect(agentProfileById(''), isNull);
  });

  test('a visible name that is no character finds nothing', () {
    expect(agentProfileFor('Zoe'), isNull);
  });
}
