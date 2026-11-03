import 'package:flutter/services.dart';

/// Keeps the screen out of screenshots, screen recordings and the recents
/// thumbnail while a pairing secret may be on it. Tests put a fake in its place.
abstract interface class SecureScreen {
  Future<void> setSecure(bool secure);
}

/// FLAG_SECURE through `dev.herdrnotify/secure_screen` (MainActivity.kt).
class MethodChannelSecureScreen implements SecureScreen {
  const MethodChannelSecureScreen();

  static const MethodChannel _channel = MethodChannel(
    'dev.herdrnotify/secure_screen',
  );

  /// A phone that cannot set the flag still scans: the failure is swallowed.
  @override
  Future<void> setSecure(bool secure) async {
    try {
      await _channel.invokeMethod<void>('setSecure', secure);
    } on PlatformException {
      // The flag is a protection, not a requirement.
    } on MissingPluginException {
      // No native side (tests, other platforms).
    }
  }
}
