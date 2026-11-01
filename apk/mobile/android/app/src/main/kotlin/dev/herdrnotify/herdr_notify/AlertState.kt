package dev.herdrnotify.herdr_notify

import android.content.Context
import java.io.File

/**
 * Flags of the alerts service, shared by the UI engine and the service engine (same process).
 * [enabled] is the saved switch «Servicio de avisos»; the rest says what runs right now.
 */
object AlertState {
    @Volatile var running = false
    @Volatile var foreground = false
    @Volatile var connected = false

    private const val PREFS = "alerts"
    private const val ENABLED = "enabled"
    private const val ENABLED_AT = "enabledAt"
    private const val BOOT_SEEN_AT = "bootSeenAt"
    // The service needs a little time after a boot before the app can say it is missing.
    private const val BOOT_GRACE_MS = 5 * 60_000L

    fun enabled(ctx: Context): Boolean = prefs(ctx).getBoolean(ENABLED, false)

    fun enabledAt(ctx: Context): Long = prefs(ctx).getLong(ENABLED_AT, 0L)

    fun bootSeenAt(ctx: Context): Long = prefs(ctx).getLong(BOOT_SEEN_AT, 0L)

    fun saveEnabled(ctx: Context, on: Boolean) {
        val editor = prefs(ctx).edit().putBoolean(ENABLED, on)
        if (on && !enabled(ctx)) editor.putLong(ENABLED_AT, System.currentTimeMillis())
        editor.apply()
    }

    fun saveBootSeen(ctx: Context) {
        prefs(ctx).edit().putLong(BOOT_SEEN_AT, System.currentTimeMillis()).apply()
    }

    /**
     * The switch was on before the last boot, the phone is unlocked, and no boot broadcast
     * reached the app since: the system never delivered it (TECNO), so the service did not come back.
     */
    fun bootMissed(
        enabled: Boolean,
        unlocked: Boolean,
        enabledAt: Long,
        bootSeenAt: Long,
        now: Long,
        elapsed: Long,
    ): Boolean {
        val bootTime = now - elapsed
        return enabled && unlocked && enabledAt < bootTime && bootSeenAt < bootTime &&
            now - bootTime > BOOT_GRACE_MS
    }

    fun log(ctx: Context) = AlertLog(File(ctx.filesDir, "alerts-diag.log"))

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}
