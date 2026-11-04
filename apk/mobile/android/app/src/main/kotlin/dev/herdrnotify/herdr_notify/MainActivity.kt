package dev.herdrnotify.herdr_notify

import android.view.WindowManager
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        flutterEngine.plugins.add(HerdrAlertsPlugin())
        flutterEngine.plugins.add(HerdrContactsPlugin())
        flutterEngine.plugins.add(HerdrDiscoveryPlugin())
        registerSecureScreen(flutterEngine)
    }

    // FLAG_SECURE keeps a pairing QR out of screenshots, recordings and the recents thumbnail.
    private fun registerSecureScreen(flutterEngine: FlutterEngine) {
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.herdrnotify/secure_screen")
            .setMethodCallHandler { call, result ->
                if (call.method != "setSecure") {
                    result.notImplemented()
                    return@setMethodCallHandler
                }
                if (call.arguments == true) {
                    window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
                } else {
                    window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
                }
                result.success(null)
            }
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
