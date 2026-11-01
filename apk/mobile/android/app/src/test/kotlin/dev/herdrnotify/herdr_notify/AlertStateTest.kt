package dev.herdrnotify.herdr_notify

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AlertStateTest {
    private val now = 1_000_000_000L
    private val tenMinutes = 600_000L
    // The boot happened ten minutes before `now`.
    private val bootTime = now - tenMinutes

    @Test
    fun `missed when the switch was on before a boot the app never heard of`() {
        assertTrue(missed(elapsed = tenMinutes))
    }

    @Test
    fun `not missed when the boot broadcast arrived`() {
        assertFalse(missed(elapsed = tenMinutes, bootSeenAt = bootTime + 1_000))
    }

    @Test
    fun `not missed when the switch was turned on after the boot`() {
        assertFalse(missed(elapsed = tenMinutes, enabledAt = bootTime + 1_000))
    }

    @Test
    fun `not missed within five minutes of the boot`() {
        assertFalse(missed(elapsed = 4 * 60_000L))
    }

    @Test
    fun `not missed while the switch is off or the phone is still locked`() {
        assertFalse(missed(elapsed = tenMinutes, enabled = false))
        assertFalse(missed(elapsed = tenMinutes, unlocked = false))
    }

    private fun missed(
        elapsed: Long,
        enabled: Boolean = true,
        unlocked: Boolean = true,
        enabledAt: Long = 0L,
        bootSeenAt: Long = 0L,
    ) = AlertState.bootMissed(enabled, unlocked, enabledAt, bootSeenAt, now, elapsed)
}
