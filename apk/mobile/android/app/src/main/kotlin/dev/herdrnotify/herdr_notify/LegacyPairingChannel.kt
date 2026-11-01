package dev.herdrnotify.herdr_notify

import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import java.io.File
import java.security.KeyStore

// `preferencesDataStore(name = "pairing")` in PairingStore.kt.
private val Context.legacyPairing: DataStore<Preferences> by preferencesDataStore(name = "pairing")

/** What the channel reads from the old app's storage. Tests put a fake in its place. */
internal interface LegacyStore {
    /** Base URL and device id, or null when the old app never paired. */
    suspend fun address(): Map<String, String>?

    /** The recovery code, or null when the old app kept none. */
    fun recoveryCode(): String?

    /** Removes the old pairing. */
    suspend fun clear()
}

/**
 * Reads the pairing the Kotlin v1 app left in this app's data directory.
 *
 * The same applicationId and signing key mean the update keeps these files, so
 * the Flutter side can offer to reuse the pairing. Every name in
 * [AndroidLegacyStore] is the one `PairingStore.kt`, `RecoveryCodeStore.kt` and
 * `DeviceKeyStore.kt` used.
 *
 * Only [LegacyStore.clear] writes, and Dart calls it after the new pairing is verified.
 */
class LegacyPairingChannel internal constructor(
    private val store: LegacyStore,
    private val postToMain: (Runnable) -> Unit,
) : MethodChannel.MethodCallHandler {

    constructor(context: Context) : this(AndroidLegacyStore(context), mainThreadPost())

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    fun register(messenger: BinaryMessenger) {
        MethodChannel(messenger, CHANNEL).setMethodCallHandler(this)
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        val work: (suspend () -> Any?)? = when (call.method) {
            METHOD_READ -> store::address
            METHOD_RECOVERY_CODE -> store::recoveryCode
            METHOD_CLEAR -> store::clear
            else -> null
        }
        if (work == null) {
            result.notImplemented()
            return
        }
        scope.launch {
            val reply = runCatching { work() }
            postToMain(
                Runnable {
                    reply.fold(
                        onSuccess = { result.success(it.toCodable()) },
                        // The exception class name only: its message can echo file contents.
                        onFailure = { result.error(ERROR_CODE, it.javaClass.simpleName, null) },
                    )
                },
            )
        }
    }

    /**
     * Every success reply passes through here. A Kotlin function without a result
     * returns [Unit], which StandardMethodCodec cannot encode; Dart gets null instead.
     */
    private fun Any?.toCodable(): Any? = if (this == Unit) null else this

    private companion object {
        const val CHANNEL = "dev.herdrnotify/legacy_pairing"
        const val METHOD_READ = "read"
        const val METHOD_RECOVERY_CODE = "recoveryCode"
        const val METHOD_CLEAR = "clear"
        const val ERROR_CODE = "legacy_pairing"

        fun mainThreadPost(): (Runnable) -> Unit {
            val mainThread = Handler(Looper.getMainLooper())
            return { mainThread.post(it) }
        }
    }
}

/** The old app's files on this device. */
internal class AndroidLegacyStore(private val context: Context) : LegacyStore {

    override suspend fun address(): Map<String, String>? {
        val prefs = context.legacyPairing.data.first()
        val baseUrl = prefs[BASE_URL_KEY].orEmpty()
        val deviceId = prefs[DEVICE_ID_KEY].orEmpty()
        if (baseUrl.isBlank() || deviceId.isBlank()) return null
        return mapOf("baseUrl" to baseUrl, "deviceId" to deviceId)
    }

    /**
     * Read only when Dart asks for it at the moment it re-enrols. Looking at the
     * file first keeps a phone that never paired from creating a master key it
     * has no use for.
     */
    override fun recoveryCode(): String? {
        if (!recoveryFile().exists()) return null
        return recoveryPrefs().getString(RECOVERY_KEY, null)?.takeIf { it.isNotBlank() }
    }

    override suspend fun clear() {
        context.legacyPairing.edit { it.clear() }
        context.deleteSharedPreferences(RECOVERY_FILE)
        deleteLegacySigningKey()
    }

    /**
     * The host has already replaced this key's public half during re-enrolment,
     * so the Keystore entry can no longer sign anything the host accepts.
     */
    private fun deleteLegacySigningKey() {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        if (keyStore.containsAlias(SIGNING_ALIAS)) keyStore.deleteEntry(SIGNING_ALIAS)
    }

    /** `shared_prefs/herdr_recovery.xml`, where SharedPreferences keeps the file. */
    private fun recoveryFile() = File(context.applicationInfo.dataDir, "shared_prefs/$RECOVERY_FILE.xml")

    private fun recoveryPrefs() = EncryptedSharedPreferences.create(
        context,
        RECOVERY_FILE,
        MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build(),
        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
    )

    private companion object {
        val BASE_URL_KEY = stringPreferencesKey("base_url")
        val DEVICE_ID_KEY = stringPreferencesKey("device_id")

        // EncryptedSharedPreferences("herdr_recovery") in RecoveryCodeStore.kt.
        const val RECOVERY_FILE = "herdr_recovery"
        const val RECOVERY_KEY = "recovery_code"

        // DeviceKeyStore.DEFAULT_ALIAS, and the AndroidKeyStore provider name.
        const val SIGNING_ALIAS = "herdr-notify-signing"
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
    }
}
