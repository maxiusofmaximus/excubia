import 'package:excubia/core/api/secure_screen.dart';

/// Records every flag change; the flag itself only exists on a device.
class FakeSecureScreen implements SecureScreen {
  final List<bool> calls = [];

  /// The last value asked for; null before any call.
  bool? get secure => calls.isEmpty ? null : calls.last;

  @override
  Future<void> setSecure(bool secure) async => calls.add(secure);
}
