package dev.excubia.app

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.provider.ContactsContract
import android.provider.Settings
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.PluginRegistry

/**
 * The phone's e-mail addresses for the agenda import, and its one permission
 * question. Only READ_CONTACTS is declared: the pub.dev contacts plugin also
 * declares WRITE_CONTACTS, which this app must not hold. Registered in MainActivity.
 */
class HerdrContactsPlugin : FlutterPlugin, MethodChannel.MethodCallHandler, ActivityAware,
    PluginRegistry.RequestPermissionsResultListener {
    private var channel: MethodChannel? = null
    private var context: Context? = null
    private var activity: Activity? = null
    private var binding: ActivityPluginBinding? = null
    private var pending: MethodChannel.Result? = null

    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        context = binding.applicationContext
        channel = MethodChannel(binding.binaryMessenger, CHANNEL).also {
            it.setMethodCallHandler(this)
        }
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        channel?.setMethodCallHandler(null)
        channel = null
        context = null
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        val ctx = context
        if (ctx == null) {
            result.error("detached", "contacts plugin is not attached", null)
            return
        }
        when (call.method) {
            "requestAccess" -> requestAccess(ctx, result)
            "readEmails" -> readEmails(ctx, result)
            "openAppSettings" -> {
                openAppSettings(ctx)
                result.success(null)
            }
            else -> result.notImplemented()
        }
    }

    /** Answers at once when the permission is already granted; otherwise asks the system. */
    private fun requestAccess(ctx: Context, result: MethodChannel.Result) {
        if (ctx.checkSelfPermission(READ_CONTACTS) == PackageManager.PERMISSION_GRANTED) {
            result.success(ANSWER_GRANTED)
            return
        }
        val current = activity
        if (current == null || pending != null) {
            result.error("no_activity", "no screen to ask the contacts permission from", null)
            return
        }
        pending = result
        current.requestPermissions(arrayOf(READ_CONTACTS), REQUEST_CODE)
    }

    /** The system's answer. A denial that the system will not ask about again is `deniedForever`. */
    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ): Boolean {
        if (requestCode != REQUEST_CODE) return false
        val answer = pending ?: return true
        pending = null
        answer.success(
            when {
                grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED -> ANSWER_GRANTED
                activity?.shouldShowRequestPermissionRationale(READ_CONTACTS) == true -> ANSWER_DENIED
                else -> ANSWER_DENIED_FOREVER
            }
        )
        return true
    }

    /** The query runs off the UI thread: a phone can hold thousands of contacts. */
    private fun readEmails(ctx: Context, result: MethodChannel.Result) {
        if (ctx.checkSelfPermission(READ_CONTACTS) != PackageManager.PERMISSION_GRANTED) {
            result.error("denied", "READ_CONTACTS is not granted", null)
            return
        }
        val main = Handler(Looper.getMainLooper())
        Thread {
            val rows = queryEmails(ctx)
            main.post { result.success(rows) }
        }.start()
    }

    private fun queryEmails(ctx: Context): List<Map<String, String?>> {
        val rows = mutableListOf<Map<String, String?>>()
        val projection = arrayOf(
            ContactsContract.CommonDataKinds.Email.CONTACT_ID,
            ContactsContract.CommonDataKinds.Email.DISPLAY_NAME_PRIMARY,
            ContactsContract.CommonDataKinds.Email.ADDRESS,
        )
        ctx.contentResolver.query(ContactsContract.CommonDataKinds.Email.CONTENT_URI, projection, null, null, null)
            ?.use { cursor ->
                while (cursor.moveToNext()) {
                    val address = cursor.getString(2) ?: continue
                    rows += mapOf(
                        "contactId" to cursor.getString(0),
                        "name" to cursor.getString(1),
                        "address" to address,
                    )
                }
            }
        return rows
    }

    private fun openAppSettings(ctx: Context) {
        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${ctx.packageName}"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        ctx.startActivity(intent)
    }

    override fun onAttachedToActivity(binding: ActivityPluginBinding) = attach(binding)

    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) = attach(binding)

    override fun onDetachedFromActivityForConfigChanges() = detach()

    override fun onDetachedFromActivity() = detach()

    private fun attach(binding: ActivityPluginBinding) {
        activity = binding.activity
        this.binding = binding
        binding.addRequestPermissionsResultListener(this)
    }

    private fun detach() {
        binding?.removeRequestPermissionsResultListener(this)
        binding = null
        activity = null
    }

    companion object {
        const val CHANNEL = "dev.excubia/contacts"
        private const val REQUEST_CODE = 7002
        private const val READ_CONTACTS = "android.permission.READ_CONTACTS"
        private const val ANSWER_GRANTED = "granted"
        private const val ANSWER_DENIED = "denied"
        private const val ANSWER_DENIED_FOREVER = "deniedForever"
    }
}
