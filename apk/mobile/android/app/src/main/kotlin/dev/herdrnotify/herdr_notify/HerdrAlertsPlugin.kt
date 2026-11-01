package dev.herdrnotify.herdr_notify

import android.Manifest
import android.app.Activity
import android.app.NotificationManager
import android.content.ActivityNotFoundException
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.os.SystemClock
import android.os.UserManager
import android.provider.Settings
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.util.Locale

/**
 * The alerts channel (`dev.herdrnotify/alerts`). The UI engine uses it for the setup screen;
 * the service engine uses it to post alerts, report the connection and write diagnostics.
 * The UI engine also opens the app's settings and asks for the notification permission,
 * which need the screen in front.
 */
class HerdrAlertsPlugin : FlutterPlugin, MethodChannel.MethodCallHandler, ActivityAware {
    private var channel: MethodChannel? = null
    private var context: Context? = null
    private var activity: Activity? = null

    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        context = binding.applicationContext
        channel = MethodChannel(binding.binaryMessenger, CHANNEL).also { it.setMethodCallHandler(this) }
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        channel?.setMethodCallHandler(null)
        channel = null
        context = null
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        val ctx = context
        if (ctx == null) {
            result.error("detached", "alerts plugin is not attached", null)
            return
        }
        when (call.method) {
            "status" -> result.success(status(ctx))
            "setEnabled" -> {
                setEnabled(ctx, call.argument<Boolean>("enabled") == true)
                result.success(null)
            }
            "openBatterySettings" -> {
                ctx.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS).newTaskFlag())
                result.success(null)
            }
            "openAutostartSettings" -> result.success(openAutostartSettings(ctx))
            "openChannelSettings" -> {
                val id = call.argument<String>("channel").orEmpty()
                if (id !in AlertNotifier.CHANNELS) return result.error("channel", "unknown channel", null)
                ctx.startActivity(
                    Intent(Settings.ACTION_CHANNEL_NOTIFICATION_SETTINGS)
                        .putExtra(Settings.EXTRA_APP_PACKAGE, ctx.packageName)
                        .putExtra(Settings.EXTRA_CHANNEL_ID, id)
                        .newTaskFlag(),
                )
                result.success(null)
            }
            "notify" -> {
                val id = call.argument<String>("channel").orEmpty()
                if (id !in AlertNotifier.CHANNELS) return result.error("channel", "unknown channel", null)
                val shown = AlertNotifier.alert(
                    ctx,
                    id,
                    call.argument<String>("title").orEmpty(),
                    call.argument<String>("body").orEmpty(),
                )
                result.success(shown)
            }
            "connection" -> {
                AlertState.connected = call.argument<Boolean>("connected") == true
                AlertNotifier.updatePermanent(ctx)
                result.success(null)
            }
            "record" -> {
                AlertState.log(ctx).append(call.argument<String>("line").orEmpty())
                result.success(null)
            }
            "readLog" -> result.success(AlertState.log(ctx).read())
            "openAppSettings" -> {
                ctx.startActivity(
                    Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${ctx.packageName}")).newTaskFlag(),
                )
                result.success(null)
            }
            "requestNotificationPermission" -> {
                requestNotificationPermission()
                result.success(null)
            }
            else -> result.notImplemented()
        }
    }

    override fun onAttachedToActivity(binding: ActivityPluginBinding) {
        activity = binding.activity
    }

    override fun onDetachedFromActivityForConfigChanges() {
        activity = null
    }

    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) {
        activity = binding.activity
    }

    override fun onDetachedFromActivity() {
        activity = null
    }

    /** Only the UI engine has an activity: the service engine asks for nothing. */
    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT >= 33) {
            activity?.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), PERMISSION_REQUEST)
        }
    }

    private fun setEnabled(ctx: Context, on: Boolean) {
        AlertState.saveEnabled(ctx, on)
        if (on) {
            AlertService.start(ctx)
            AlertWatchdog.schedule(ctx)
        } else {
            AlertService.stop(ctx)
            AlertWatchdog.cancel(ctx)
        }
    }

    private fun status(ctx: Context): Map<String, Any?> {
        AlertNotifier.ensureChannels(ctx)
        return mapOf(
            "notificationsGranted" to ctx.getSystemService(NotificationManager::class.java).areNotificationsEnabled(),
            "channels" to AlertNotifier.CHANNELS.associateWith { AlertNotifier.isActive(ctx, it) },
            "batteryExempt" to ctx.getSystemService(PowerManager::class.java)
                .isIgnoringBatteryOptimizations(ctx.packageName),
            "serviceRunning" to AlertState.running,
            "enabled" to AlertState.enabled(ctx),
            "lastExit" to ExitReasons.lastText(ctx),
            "manufacturer" to Build.MANUFACTURER.lowercase(Locale.ROOT),
            "fgsType" to BuildConfig.ALERTS_FGS_TYPE,
            "autostartScreen" to (ctx.packageManager.resolveActivity(autostartComponent(), 0) != null),
            "bootMissed" to AlertState.bootMissed(
                enabled = AlertState.enabled(ctx),
                unlocked = ctx.getSystemService(UserManager::class.java).isUserUnlocked,
                enabledAt = AlertState.enabledAt(ctx),
                bootSeenAt = AlertState.bootSeenAt(ctx),
                now = System.currentTimeMillis(),
                elapsed = SystemClock.elapsedRealtime(),
            ),
        )
    }

    /** True when Phone Master's autostart screen opened; otherwise the app's own settings did. */
    private fun openAutostartSettings(ctx: Context): Boolean {
        for (intent in listOf(autostartComponent(), Intent(AUTOSTART_ACTION))) {
            try {
                ctx.startActivity(intent.newTaskFlag())
                return true
            } catch (e: ActivityNotFoundException) {
                // This build has no such screen: try the next one.
            } catch (e: SecurityException) {
                // The phone's manager refuses outside callers: try the next one.
            }
        }
        ctx.startActivity(
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${ctx.packageName}")).newTaskFlag(),
        )
        return false
    }

    private fun autostartComponent() = Intent().setComponent(ComponentName(PHONE_MASTER, AUTOSTART_CLASS))

    /** Settings screens are opened from the application context, which needs this flag. */
    private fun Intent.newTaskFlag(): Intent = addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    companion object {
        const val CHANNEL = "dev.herdrnotify/alerts"
        private const val PERMISSION_REQUEST = 7001
        private const val PHONE_MASTER = "com.transsion.phonemaster"
        private const val AUTOSTART_CLASS = "com.cyin.himgr.autostart.AutoStartActivity"
        private const val AUTOSTART_ACTION = "com.cyin.himgr.applicationmanager.view.activities.AUTO_START_ACTIVITY"
    }
}
