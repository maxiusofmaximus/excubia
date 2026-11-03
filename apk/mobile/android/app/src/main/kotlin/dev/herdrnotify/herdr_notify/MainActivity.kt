package dev.herdrnotify.herdr_notify

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        LegacyPairingChannel(applicationContext).register(flutterEngine.dartExecutor.binaryMessenger)
        flutterEngine.plugins.add(HerdrAlertsPlugin())
        flutterEngine.plugins.add(HerdrContactsPlugin())
        flutterEngine.plugins.add(HerdrDiscoveryPlugin())
    }

    // Alerts stay silent while the screen is in front: the app already shows them.
    override fun onStart() {
        super.onStart()
        AlertState.foreground = true
        // Some phones (TECNO) never deliver BOOT_COMPLETED: opening the app brings the service back.
        if (AlertState.enabled(this) && !AlertState.running) {
            AlertService.start(this)
            AlertWatchdog.schedule(this)
        }
    }

    override fun onStop() {
        AlertState.foreground = false
        super.onStop()
    }
}
