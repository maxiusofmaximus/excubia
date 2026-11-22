import 'package:flutter_test/flutter_test.dart';
import 'package:excubia/core/state/future_lru.dart';

void main() {
  test('una clave cargada no vuelve a pedirse mientras esté en la caché', () async {
    var loads = 0;
    final cache = FutureLru<String, int>((key) async => ++loads, capacity: 2);

    expect(await cache.load('a'), 1);
    expect(await cache.load('a'), 1);
    expect(loads, 1);
  });

  test('pasado el tope sale la clave usada hace más tiempo', () async {
    var loads = 0;
    final cache = FutureLru<String, int>((key) async => ++loads, capacity: 2);

    await cache.load('a');
    await cache.load('b');
    await cache.load('a'); // «a» pasa a ser la más reciente
    await cache.load('c'); // sale «b»
    expect(loads, 3);

    await cache.load('a');
    expect(loads, 3, reason: '«a» sigue en la caché');
    await cache.load('b');
    expect(loads, 4, reason: '«b» tuvo que volver a pedirse');
  });

  test('forget olvida una clave para que la siguiente carga vuelva a pedirla', () async {
    var loads = 0;
    final cache = FutureLru<String, int>((key) async => ++loads, capacity: 2);

    await cache.load('a');
    cache.forget('a');
    await cache.load('a');
    expect(loads, 2);
  });
}
