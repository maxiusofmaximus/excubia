package dev.excubia.app

import android.app.ApplicationExitInfo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ExitReasonsTest {
    @Test
    fun `names the common stops in Spanish`() {
        assertEquals("la app dejó de responder", ExitReasons.text(ApplicationExitInfo.REASON_ANR))
        assertEquals("el sistema liberó memoria", ExitReasons.text(ApplicationExitInfo.REASON_LOW_MEMORY))
        assertEquals("cerrada por el usuario", ExitReasons.text(ApplicationExitInfo.REASON_USER_REQUESTED))
    }

    @Test
    fun `an unknown code still says which code it was`() {
        assertTrue(ExitReasons.text(999).contains("999"))
    }
}
