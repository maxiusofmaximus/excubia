package dev.herdrnotify.herdr_notify

import java.net.InetAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RawMdnsProbeTest {
    // Real response captured from the host (random instance, LAN ip and port, no secrets).
    private val fixture: ByteArray = FIXTURE_HEX.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
    private val lan = InetAddress.getByName("192.168.50.99")

    /** Port written in the fixture's own SRV record: type 33, class IN, ttl, rdlen, priority, weight, port. */
    private val fixturePort = (12 until fixture.size - 1)
        .first { i -> fixture[i].toInt() == 0 && fixture[i + 1].toInt() == 33 && fixture[i + 3].toInt() == 1 }
        .let { ((fixture[it + 14].toInt() and 0xFF) shl 8) or (fixture[it + 15].toInt() and 0xFF) }

    private fun ByteArray.indexOfSequence(s: ByteArray) = (0..size - s.size).first { i -> s.indices.all { this[i + it] == s[it] } }

    private fun withInstance(name: String): ByteArray {
        val old = "excubia-0f0cb349".toByteArray()
        val copy = fixture.copyOf()
        name.toByteArray().copyInto(copy, copy.indexOfSequence(old))
        return copy
    }

    @Test
    fun `reads the A record of the SRV target and the port of the SRV`() {
        assertEquals("192.168.50.147:$fixturePort", RawMdnsProbe.parseAnswer(fixture, lan))
    }

    @Test
    fun `the A record wins over the packet source`() {
        // source differs from the A record: the announced address is the right one
        assertEquals("192.168.50.147:$fixturePort", RawMdnsProbe.parseAnswer(fixture, InetAddress.getByName("10.0.0.7")))
    }

    @Test
    fun `without a usable A record the source ip is used`() {
        val noA = fixture.copyOf()
        // turn the A record (type 1) into type 16 (TXT-like, ignored)
        val at = noA.size - 14 // name(2) type(2) ...: type bytes are 14 from the end
        noA[at + 1] = 16
        assertEquals("10.0.0.7:$fixturePort", RawMdnsProbe.parseAnswer(noA, InetAddress.getByName("10.0.0.7")))
    }

    @Test
    fun `truncated packet gives null`() {
        assertNull(RawMdnsProbe.parseAnswer(fixture.copyOf(fixture.size / 2), lan))
    }

    @Test
    fun `another instance gives null`() {
        assertNull(RawMdnsProbe.parseAnswer(withInstance("other-0123456789"), lan))
    }

    @Test
    fun `a query is not an answer`() {
        assertNull(RawMdnsProbe.parseAnswer(RawMdnsProbe.buildQuery(), lan))
        val flagsOff = fixture.copyOf().also { it[2] = (it[2].toInt() and 0x7F).toByte() }
        assertNull(RawMdnsProbe.parseAnswer(flagsOff, lan))
    }

    @Test
    fun `a compression pointer to itself does not hang`() {
        val loop = fixture.copyOf()
        loop[12] = 0xC0.toByte() // the first question name now points at offset 12 = itself
        loop[13] = 12
        assertNull(RawMdnsProbe.parseAnswer(loop, lan))
    }

    @Test
    fun `garbage never throws`() {
        assertNull(RawMdnsProbe.parseAnswer(ByteArray(0), lan))
        assertNull(RawMdnsProbe.parseAnswer(ByteArray(40) { 0xFF.toByte() }, lan))
    }

    private companion object {
        const val FIXTURE_HEX =
            "0000840000010001000000030d5f657863756269612d70616972045f746370056c6f63616c00000c0001c00c000c00010000000a001310657863756269612d3066306362333439c00cc036002100010000000a001900000000b95310657863756269612d3066306362333439c01fc036001000010000000a000403763d31c05b000100010000000a0004c0a83293"
    }
}
