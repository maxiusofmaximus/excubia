import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/voice_recorder.dart';

void main() {
  test('el nivel va de 0 a 1: -60 dB o menos es silencio, 0 dB el máximo', () {
    expect(micLevel(-160), 0);
    expect(micLevel(-60), 0);
    expect(micLevel(-30), 0.5);
    expect(micLevel(0), 1);
    expect(micLevel(12), 1);
  });
}
