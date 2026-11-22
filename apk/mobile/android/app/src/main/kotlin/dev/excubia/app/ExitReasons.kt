package dev.excubia.app

import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.Context
import android.os.Build

/** Why the system last stopped the app, in Spanish (ApplicationExitInfo, Android 11+). */
object ExitReasons {
    fun text(reason: Int): String = when (reason) {
        ApplicationExitInfo.REASON_USER_REQUESTED -> "cerrada por el usuario"
        ApplicationExitInfo.REASON_USER_STOPPED -> "detenida desde los ajustes"
        ApplicationExitInfo.REASON_LOW_MEMORY -> "el sistema liberó memoria"
        ApplicationExitInfo.REASON_ANR -> "la app dejó de responder"
        ApplicationExitInfo.REASON_CRASH, ApplicationExitInfo.REASON_CRASH_NATIVE -> "fallo de la app"
        ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE -> "uso excesivo de recursos"
        ApplicationExitInfo.REASON_DEPENDENCY_DIED -> "se detuvo un componente del que depende"
        ApplicationExitInfo.REASON_EXIT_SELF -> "la app se cerró sola"
        ApplicationExitInfo.REASON_SIGNALED -> "señal del sistema"
        ApplicationExitInfo.REASON_PERMISSION_CHANGE -> "cambió un permiso"
        ApplicationExitInfo.REASON_OTHER -> "otra causa"
        else -> "causa desconocida ($reason)"
    }

    /** The newest stop the system recorded, or null when there is none or Android is older than 11. */
    fun last(ctx: Context): ApplicationExitInfo? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return null
        return ctx.getSystemService(ActivityManager::class.java)
            .getHistoricalProcessExitReasons(ctx.packageName, 0, 1)
            .firstOrNull()
    }

    fun lastText(ctx: Context): String = when {
        Build.VERSION.SDK_INT < Build.VERSION_CODES.R -> "no disponible en esta versión de Android"
        else -> last(ctx)?.let { text(it.reason) } ?: "sin datos"
    }
}
