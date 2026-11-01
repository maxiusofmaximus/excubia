package dev.herdrnotify.herdr_notify

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit

/** Periodic check (WorkManager's minimum is 15 minutes) that restarts the alerts service. */
object AlertWatchdog {
    private const val NAME = "herdr-alerts-watchdog"

    fun schedule(ctx: Context) {
        val request = PeriodicWorkRequestBuilder<AlertWorker>(15, TimeUnit.MINUTES).build()
        WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
    }

    fun cancel(ctx: Context) {
        WorkManager.getInstance(ctx).cancelUniqueWork(NAME)
    }
}

class AlertWorker(ctx: Context, params: WorkerParameters) : Worker(ctx, params) {
    override fun doWork(): Result {
        if (AlertState.enabled(applicationContext) && !AlertState.running) {
            AlertService.start(applicationContext)
        }
        return Result.success()
    }
}

/** After a reboot the service comes back if the switch was on. */
class AlertBoot : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
        val on = AlertState.enabled(context)
        // The diagnostic tells apart a boot the system never delivered from a switch found off.
        AlertState.log(context).append("reinicio del móvil: interruptor ${if (on) "encendido" else "apagado"}")
        AlertState.saveBootSeen(context)
        if (!on) return
        AlertService.start(context)
        AlertWatchdog.schedule(context)
    }
}
