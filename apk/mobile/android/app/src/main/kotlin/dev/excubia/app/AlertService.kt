package dev.excubia.app

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.dart.DartExecutor
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugins.GeneratedPluginRegistrant

/**
 * Keeps the stream to the PC open while the app is closed, as a foreground service. The stream
 * runs in a Dart engine with the same code as the open app; this class owns the service: the
 * permanent notice, the network callback and the restart rules. It never stops on a lost network.
 */
class AlertService : Service() {
    private var engine: FlutterEngine? = null
    private var channel: MethodChannel? = null
    private var network: ConnectivityManager.NetworkCallback? = null
    private val main = Handler(Looper.getMainLooper())

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startInForeground()
        AlertState.running = true
        if (engine == null) startEngine()
        if (network == null) watchNetwork()
        return START_STICKY
    }

    /** Android 15: a dataSync or specialUse service ran its six hours. Stop it and start again. */
    override fun onTimeout(startId: Int, fgsType: Int) {
        AlertState.log(this).append("tiempo agotado (tipo $fgsType): reinicio")
        stopSelf(startId)
        start(this)
    }

    override fun onDestroy() {
        network?.let { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(it) }
        network = null
        engine?.destroy()
        engine = null
        channel = null
        AlertState.running = false
        AlertState.connected = false
        AlertState.log(this).append("servicio parado")
        super.onDestroy()
    }

    private fun startInForeground() {
        AlertNotifier.ensureChannels(this)
        val notice = AlertNotifier.permanent(this, AlertState.connected)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(AlertNotifier.PERMANENT_ID, notice, serviceType(BuildConfig.ALERTS_FGS_TYPE))
        } else {
            startForeground(AlertNotifier.PERMANENT_ID, notice)
        }
    }

    private fun startEngine() {
        val created = FlutterEngine(this).apply {
            GeneratedPluginRegistrant.registerWith(this)
            plugins.add(HerdrAlertsPlugin())
            dartExecutor.executeDartEntrypoint(DartExecutor.DartEntrypoint.createDefault(), listOf(BACKGROUND_ARG))
        }
        engine = created
        channel = MethodChannel(created.dartExecutor.binaryMessenger, HerdrAlertsPlugin.CHANNEL)
        AlertState.log(this).append("arranque; última parada: ${ExitReasons.lastText(this)}")
    }

    /** A network that comes back wakes the stream at once instead of after its backoff. */
    private fun watchNetwork() {
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                // Callbacks arrive on a binder thread; the channel needs the main thread.
                main.post { channel?.invokeMethod("networkUp", null) }
            }
        }
        getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(callback)
        network = callback
    }

    companion object {
        /** Flag the engine passes to its entrypoint; `main` checks it and skips the UI. */
        const val BACKGROUND_ARG = "--herdr-alerts-bg"

        /** A start the system refuses is logged, not thrown: the watchdog tries again. */
        fun start(ctx: Context) {
            val intent = Intent(ctx, AlertService::class.java)
            try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) ctx.startForegroundService(intent)
                else ctx.startService(intent)
            } catch (e: IllegalStateException) {
                AlertState.log(ctx).append("arranque no permitido: ${e.javaClass.simpleName}")
            }
        }

        fun stop(ctx: Context) {
            ctx.stopService(Intent(ctx, AlertService::class.java))
        }

        private fun serviceType(name: String): Int = when (name) {
            "connectedDevice" -> ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE
            "dataSync" -> ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
            else -> ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
        }
    }
}
