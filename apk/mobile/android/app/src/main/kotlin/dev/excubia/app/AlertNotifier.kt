package dev.excubia.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.os.Build
import android.os.PowerManager

/**
 * The four alert channels and the two notices: the permanent one of the service and the
 * alerts. Alerts reuse the local alert's id, so a newer one replaces the older instead of stacking.
 */
object AlertNotifier {
    const val CH_APPROVALS = "agents"
    const val CH_REVIEW = "agents-review"
    const val CH_REMINDERS = "recordatorios"
    const val CH_CONNECTION = "conexion"
    val CHANNELS = listOf(CH_APPROVALS, CH_REVIEW, CH_REMINDERS, CH_CONNECTION)

    const val PERMANENT_ID = 2
    private const val ALERT_ID = 1
    private const val WAKE_MILLIS = 10_000L
    private const val TAG = "herdr-alerts"

    fun ensureChannels(ctx: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        ctx.getSystemService(NotificationManager::class.java).createNotificationChannels(
            listOf(
                NotificationChannel(CH_APPROVALS, "Aprobaciones y preguntas", NotificationManager.IMPORTANCE_HIGH),
                NotificationChannel(CH_REVIEW, "Tareas terminadas", NotificationManager.IMPORTANCE_DEFAULT),
                NotificationChannel(CH_REMINDERS, "Recordatorios del calendario", NotificationManager.IMPORTANCE_DEFAULT),
                NotificationChannel(CH_CONNECTION, "Conexión con el PC", NotificationManager.IMPORTANCE_LOW)
                    .apply { setSound(null, null) },
            ),
        )
    }

    /** Whether the user left the channel on; before Android 8 channels do not exist. */
    fun isActive(ctx: Context, channel: String): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return true
        val found = ctx.getSystemService(NotificationManager::class.java).getNotificationChannel(channel)
        return found != null && found.importance != NotificationManager.IMPORTANCE_NONE
    }

    fun permanent(ctx: Context, connected: Boolean): Notification {
        val title = if (connected) "Excubia escucha a tu PC" else "Sin conexión con el PC"
        return builder(ctx, CH_CONNECTION)
            .setSmallIcon(ctx.applicationInfo.icon)
            .setContentTitle(title)
            .setOngoing(true)
            .build()
    }

    fun updatePermanent(ctx: Context) {
        ctx.getSystemService(NotificationManager::class.java)
            .notify(PERMANENT_ID, permanent(ctx, AlertState.connected))
    }

    /** False when the UI is in front: the screen already shows what the alert would say. */
    fun alert(ctx: Context, channel: String, title: String, body: String): Boolean {
        if (AlertState.foreground) return false
        ensureChannels(ctx)
        // The screen wakes long enough for the notice to show; the lock ends by itself.
        ctx.getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "$TAG:alert")
            .acquire(WAKE_MILLIS)
        val notification = builder(ctx, channel)
            .setSmallIcon(ctx.applicationInfo.icon)
            .setContentTitle(title)
            .setContentText(body)
            .setAutoCancel(true)
            .build()
        ctx.getSystemService(NotificationManager::class.java).notify(ALERT_ID, notification)
        return true
    }

    @Suppress("DEPRECATION")
    private fun builder(ctx: Context, channel: String): Notification.Builder =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) Notification.Builder(ctx, channel)
        else Notification.Builder(ctx)
}
