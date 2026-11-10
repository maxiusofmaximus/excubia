package dev.excubia.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

class AlertLogTest {
    @get:Rule
    val folder = TemporaryFolder()

    @Test
    fun `keeps every line in order while under the limit`() {
        val log = AlertLog(File(folder.root, "diag.log"), limit = 1024)
        log.append("uno", Date(0))
        log.append("dos", Date(0))
        val lines = log.read().lines().filter { it.isNotEmpty() }
        // The stamp is local time, so the expected one is formatted the same way.
        val at = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US).format(Date(0))
        assertEquals(listOf("$at uno", "$at dos"), lines)
        assertFalse(File(folder.root, "diag.log.1").exists())
    }

    @Test
    fun `moves the full file to dot-one and keeps writing the active one`() {
        val file = File(folder.root, "diag.log")
        val log = AlertLog(file, limit = 80)
        log.append("primera linea de relleno para llenar", Date(0))
        log.append("segunda linea de relleno para llenar", Date(0))
        log.append("tercera", Date(0))
        assertTrue(File(folder.root, "diag.log.1").exists())
        assertTrue(file.readText().endsWith("tercera\n"))
        // Only one rotated file is kept: the first line was dropped with the second rotation.
        assertTrue(log.read().contains("segunda linea"))
        assertTrue(log.read().contains("tercera"))
        assertFalse(log.read().contains("primera linea"))
    }
}
