package dev.excubia.app

import java.net.DatagramPacket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.MulticastSocket
import java.net.NetworkInterface
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Finds the pairing host with our own mDNS query, independent of NsdManager (whose state can get
 * stuck inside the app process after several searches). The query leaves from an ephemeral port,
 * so the host answers us directly by unicast (RFC 6762 section 6.7, "legacy unicast").
 */
class RawMdnsProbe(
    private val windowMs: Long = WINDOW_MS,
    private val onFound: (String) -> Unit,
) {
    private val cancelled = AtomicBoolean(false)
    private val delivered = AtomicBoolean(false)
    private val sockets = mutableListOf<MulticastSocket>()
    private val threads = mutableListOf<Thread>()

    /** Starts one worker thread per usable interface and returns at once. Never throws. */
    @Synchronized
    fun start() {
        if (cancelled.get()) return
        val addresses = try { localAddresses() } catch (_: Exception) { emptyList() }
        for ((ni, ip) in addresses) {
            val thread = Thread({ run(ni, ip) }, "excubia-mdns-probe")
            thread.isDaemon = true
            threads += thread
            thread.start()
        }
    }

    /** Closes every socket and ends the threads. Safe to call twice. */
    @Synchronized
    fun cancel() {
        cancelled.set(true)
        sockets.forEach { try { it.close() } catch (_: Exception) {} }
        sockets.clear()
        threads.forEach { it.interrupt() }
        threads.clear()
    }

    private fun run(ni: NetworkInterface, ip: Inet4Address) {
        var socket: MulticastSocket? = null
        try {
            socket = MulticastSocket(InetSocketAddress(ip, 0)).apply {
                networkInterface = ni
                timeToLive = 255
                soTimeout = POLL_MS
            }
            synchronized(this) {
                if (cancelled.get()) return
                sockets += socket
            }
            val query = buildQuery()
            val group = InetSocketAddress(InetAddress.getByName(GROUP), PORT)
            val buffer = ByteArray(BUFFER)
            val begin = System.nanoTime()
            var sent = 0
            while (!cancelled.get()) {
                val elapsed = (System.nanoTime() - begin) / 1_000_000
                if (elapsed >= windowMs) break
                if (sent < SEND_AT_MS.size && elapsed >= SEND_AT_MS[sent]) {
                    socket.send(DatagramPacket(query, query.size, group))
                    sent++
                }
                val packet = DatagramPacket(buffer, buffer.size)
                try {
                    socket.receive(packet)
                } catch (_: java.net.SocketTimeoutException) {
                    continue
                }
                val address = parseAnswer(packet.data.copyOf(packet.length), packet.address)
                if (address != null && !cancelled.get() && delivered.compareAndSet(false, true)) {
                    onFound(address)
                    break
                }
            }
        } catch (_: Exception) {
            // No Wi-Fi, no permission, socket closed by cancel(): this interface just finds nothing.
        } finally {
            try { socket?.close() } catch (_: Exception) {}
        }
    }

    companion object {
        private const val GROUP = "224.0.0.251"
        private const val PORT = 5353
        private const val BUFFER = 9000
        private const val POLL_MS = 100
        private const val WINDOW_MS = 8000L
        private val SEND_AT_MS = longArrayOf(0, 700, 1700, 3200)
        private const val SERVICE = "_excubia-pair._tcp.local"
        private const val PREFIX = "excubia-"
        private const val TYPE_A = 1
        private const val TYPE_PTR = 12
        private const val TYPE_SRV = 33
        private const val MAX_HOPS = 16
        private const val MAX_RECORDS = 64

        /** (interface, IPv4) pairs that can reach the LAN: up, multicast, private address. */
        private fun localAddresses(): List<Pair<NetworkInterface, Inet4Address>> =
            NetworkInterface.getNetworkInterfaces().toList().filter {
                it.isUp && !it.isLoopback && it.supportsMulticast()
            }.flatMap { ni ->
                ni.inetAddresses.toList().filterIsInstance<Inet4Address>()
                    .filter { it.isSiteLocalAddress }.map { ni to it }
            }

        /** Standard DNS query: one question, PTR, class IN. */
        internal fun buildQuery(): ByteArray {
            val out = java.io.ByteArrayOutputStream()
            out.write(byteArrayOf(0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0))
            SERVICE.split('.').forEach {
                out.write(it.length)
                out.write(it.toByteArray(Charsets.US_ASCII))
            }
            out.write(0)
            out.write(byteArrayOf(0, TYPE_PTR.toByte(), 0, 1))
            return out.toByteArray()
        }

        private class Name(val text: String, val end: Int)

        /** Reads a (possibly compressed) name at [start]; null when malformed, truncated or looping. */
        private fun readName(p: ByteArray, start: Int): Name? {
            val labels = ArrayList<String>()
            var pos = start
            var end = -1
            var hops = 0
            var total = 0
            while (true) {
                if (pos >= p.size) return null
                val len = p[pos].toInt() and 0xFF
                when {
                    len == 0 -> {
                        if (end < 0) end = pos + 1
                        return Name(labels.joinToString(".").lowercase(), end)
                    }
                    len and 0xC0 == 0xC0 -> {
                        if (pos + 1 >= p.size || ++hops > MAX_HOPS) return null
                        if (end < 0) end = pos + 2
                        pos = ((len and 0x3F) shl 8) or (p[pos + 1].toInt() and 0xFF)
                    }
                    len and 0xC0 != 0 -> return null
                    else -> {
                        total += len + 1
                        if (pos + 1 + len > p.size || total > 255) return null
                        labels += String(p, pos + 1, len, Charsets.US_ASCII)
                        pos += 1 + len
                    }
                }
            }
        }

        private fun u16(p: ByteArray, o: Int) = ((p[o].toInt() and 0xFF) shl 8) or (p[o + 1].toInt() and 0xFF)

        /**
         * `ip:port` of the pairing host in an mDNS response, or null for anything else (queries,
         * other services, truncated or malformed packets). Never throws.
         */
        fun parseAnswer(packet: ByteArray, from: InetAddress?): String? = try {
            parse(packet, from)
        } catch (_: RuntimeException) {
            null
        }

        private fun parse(p: ByteArray, from: InetAddress?): String? {
            if (p.size < 12 || (u16(p, 2) and 0x8000) == 0) return null // not a response
            var pos = 12
            repeat(u16(p, 4)) { // skip the questions
                pos = (readName(p, pos) ?: return null).end + 4
            }
            val records = minOf(u16(p, 6) + u16(p, 8) + u16(p, 10), MAX_RECORDS)
            var instanceHost: String? = null // SRV target of our instance
            var port = 0
            val ipv4 = HashMap<String, InetAddress>()
            repeat(records) {
                val owner = readName(p, pos) ?: return null
                val fixed = owner.end
                if (fixed + 10 > p.size) return null
                val type = u16(p, fixed)
                val dataStart = fixed + 10
                val dataEnd = dataStart + u16(p, fixed + 8)
                if (dataEnd > p.size) return null
                when (type) {
                    TYPE_SRV -> if (owner.text.startsWith(PREFIX) && owner.text.endsWith(".$SERVICE") && dataStart + 6 <= dataEnd) {
                        val srvPort = u16(p, dataStart + 4)
                        val target = readName(p, dataStart + 6)
                        if (srvPort > 0 && target != null) {
                            instanceHost = target.text
                            port = srvPort
                        }
                    }
                    TYPE_A -> if (dataEnd - dataStart == 4) {
                        ipv4[owner.text] = InetAddress.getByAddress(p.copyOfRange(dataStart, dataEnd))
                    }
                }
                pos = dataEnd
            }
            val host = instanceHost ?: return null
            val ip = ipv4[host]?.takeIf { usable(it) && !it.isLinkLocalAddress }
                ?: from?.takeIf { usable(it) }
                ?: return null
            return "${ip.hostAddress}:$port"
        }

        private fun usable(a: InetAddress) = a is Inet4Address && !a.isLoopbackAddress && !a.isAnyLocalAddress
    }
}
