package dev.herdrnotify.herdr_notify

import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.StandardMethodCodec
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * A reply the codec cannot encode (kotlin.Unit from `clear`) throws on the main
 * thread and crashes the app, so every reply is checked through the same codec
 * path `result.success` uses.
 */
class LegacyPairingChannelTest {

    @Test
    fun clearSuccessRepliesNullWhichTheCodecEncodes() {
        val reply = ask("clear", FakeStore())

        assertNull(reply.errorCode)
        assertNull(reply.value)
        assertEncodable(reply.value)
    }

    @Test
    fun readRepliesAnEncodableMap() {
        val reply = ask("read", FakeStore())

        assertEquals(mapOf("baseUrl" to "http://[::1]:18879", "deviceId" to "device-1"), reply.value)
        assertEncodable(reply.value)
    }

    @Test
    fun readWithoutAPairingRepliesNull() {
        val reply = ask("read", FakeStore(pairing = null))

        assertNull(reply.value)
        assertEncodable(reply.value)
    }

    @Test
    fun recoveryCodeRepliesAnEncodableString() {
        val reply = ask("recoveryCode", FakeStore())

        assertEquals("CODE-123", reply.value)
        assertEncodable(reply.value)
    }

    @Test
    fun recoveryCodeWithoutOneRepliesNull() {
        val reply = ask("recoveryCode", FakeStore(code = null))

        assertNull(reply.value)
        assertEncodable(reply.value)
    }

    @Test
    fun failureRepliesWithTheExceptionNameOnly() {
        val reply = ask("clear", FailingStore)

        assertEquals("legacy_pairing", reply.errorCode)
        assertEquals("IllegalStateException", reply.errorMessage)
    }

    @Test
    fun unknownMethodIsNotImplemented() {
        val reply = ask("forget", FakeStore())

        assertTrue(reply.wasNotImplemented)
    }

    private fun ask(method: String, store: LegacyStore): RecordingResult {
        val result = RecordingResult()
        LegacyPairingChannel(store) { it.run() }.onMethodCall(MethodCall(method, null), result)
        assertTrue("el canal no respondió", result.await())
        return result
    }

    /** Throws the same exception the codec would, for any value it cannot send. */
    private fun assertEncodable(value: Any?) {
        StandardMethodCodec.INSTANCE.encodeSuccessEnvelope(value)
    }

    private class RecordingResult : MethodChannel.Result {
        private val replied = CountDownLatch(1)

        var value: Any? = null
        var errorCode: String? = null
        var errorMessage: String? = null
        var wasNotImplemented = false

        override fun success(result: Any?) {
            value = result
            replied.countDown()
        }

        override fun error(code: String, message: String?, details: Any?) {
            errorCode = code
            errorMessage = message
            replied.countDown()
        }

        override fun notImplemented() {
            wasNotImplemented = true
            replied.countDown()
        }

        fun await(): Boolean = replied.await(5, TimeUnit.SECONDS)
    }

    private class FakeStore(
        private val pairing: Map<String, String>? = mapOf("baseUrl" to "http://[::1]:18879", "deviceId" to "device-1"),
        private val code: String? = "CODE-123",
    ) : LegacyStore {
        override suspend fun address(): Map<String, String>? = pairing

        override fun recoveryCode(): String? = code

        override suspend fun clear() {}
    }

    private object FailingStore : LegacyStore {
        override suspend fun address(): Map<String, String>? = throw IllegalStateException("contenido del fichero")

        override fun recoveryCode(): String? = throw IllegalStateException("contenido del fichero")

        override suspend fun clear() {
            throw IllegalStateException("contenido del fichero")
        }
    }
}
