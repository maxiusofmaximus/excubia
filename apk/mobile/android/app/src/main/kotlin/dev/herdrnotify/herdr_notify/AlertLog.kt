package dev.herdrnotify.herdr_notify

import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Local diagnostics: one line per event, never message content. The active file moves to
 * `.1` once it would pass [limit] bytes, so two files at most are kept.
 */
class AlertLog(private val file: File, private val limit: Long = 200L * 1024) {
    private val rotated = File(file.path + ".1")
    private val stamp = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US)

    @Synchronized
    fun append(line: String, now: Date = Date()) {
        val entry = "${stamp.format(now)} $line\n"
        if (file.exists() && file.length() + entry.toByteArray().size > limit) {
            rotated.delete()
            file.renameTo(rotated)
        }
        file.appendText(entry)
    }

    fun read(): String = listOf(rotated, file).filter { it.exists() }.joinToString("") { it.readText() }
}
