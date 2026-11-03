package dev.herdrnotify.herdr_notify

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import androidx.annotation.RequiresApi
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.net.Inet4Address
import java.net.InetAddress
import java.util.concurrent.Executor

/**
 * Finds the PC that is showing a pairing code: the host announces `_excubia-pair._tcp`
 * (instance `excubia-<8 hex>`, port = its TLS listener) only while a code is alive.
 * Answers a list of `ip:port` (IPv4, no link-local). Registered in MainActivity.
 */
class HerdrDiscoveryPlugin : FlutterPlugin, MethodChannel.MethodCallHandler {
    private var channel: MethodChannel? = null
    private var context: Context? = null
    private var busy = false

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
        when {
            call.method != "discover" -> result.notImplemented()
            ctx == null -> result.error(FAILED, "discovery plugin is not attached", null)
            busy -> result.error(FAILED, "a search is already running", null)
            else -> {
                busy = true
                val timeoutMs = (call.argument<Number>("timeoutMs")?.toLong() ?: DEFAULT_TIMEOUT_MS)
                Search(ctx, timeoutMs, result) { busy = false }.start()
            }
        }
    }

    /** One search. Every callback hops to the main thread, so its state needs no locks. */
    private class Search(
        ctx: Context,
        private val timeoutMs: Long,
        private val result: MethodChannel.Result,
        private val onClosed: () -> Unit,
    ) {
        private val main = Handler(Looper.getMainLooper())
        private val mainExecutor = Executor { main.post(it) }
        private val nsd = ctx.getSystemService(Context.NSD_SERVICE) as NsdManager
        private val wifi = ctx.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        private val found = LinkedHashSet<String>()
        private val queue = ArrayDeque<NsdServiceInfo>() // before API 34 only one resolve may run
        private val callbacks = mutableListOf<NsdManager.ServiceInfoCallback>()
        private val retries = HashMap<String, Int>() // failed resolutions retried, per service
        private var lock: WifiManager.MulticastLock? = null
        private var started = false
        private var resolving = false
        private var finished = false

        private val timeout = Runnable { finish(null) }

        private val discovery = object : NsdManager.DiscoveryListener {
            override fun onServiceFound(info: NsdServiceInfo) {
                main.post { if (!finished && info.serviceName.startsWith(PREFIX)) resolve(info) }
            }

            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
                main.post { finish("discovery start failed ($errorCode)") }
            }

            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {}
            override fun onDiscoveryStarted(serviceType: String) {}
            override fun onDiscoveryStopped(serviceType: String) {}
            override fun onServiceLost(info: NsdServiceInfo) {}
        }

        fun start() {
            try {
                // Without the lock many phones drop multicast packets to save battery.
                lock = wifi.createMulticastLock("excubia-pair").apply {
                    setReferenceCounted(false)
                    acquire()
                }
                started = true
                nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, discovery)
                main.postDelayed(timeout, timeoutMs)
            } catch (e: Exception) {
                started = false
                finish(e.message ?: e.javaClass.simpleName)
            }
        }

        private fun resolve(info: NsdServiceInfo) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                watch(info)
            } else {
                queue.addLast(info)
                resolveNext()
            }
        }

        @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
        private fun watch(info: NsdServiceInfo) {
            val callback = object : NsdManager.ServiceInfoCallback {
                override fun onServiceInfoCallbackRegistrationFailed(errorCode: Int) {
                    main.post { callbacks -= this; retry(info) }
                }

                override fun onServiceUpdated(info: NsdServiceInfo) {
                    val address = info.hostAddresses.firstNotNullOfOrNull { usable(it, info.port) }
                    if (address != null) accept(address)
                }

                override fun onServiceLost() {}
                override fun onServiceInfoCallbackUnregistered() {}
            }
            try {
                nsd.registerServiceInfoCallback(info, mainExecutor, callback)
                callbacks += callback
            } catch (_: Exception) {
                // This one announcement could not be followed; the others still count.
            }
        }

        /** A resolution that failed is tried again, twice at most, inside the same window. */
        private fun retry(info: NsdServiceInfo) {
            val done = retries[info.serviceName] ?: 0
            if (finished || done >= MAX_RETRIES) return
            retries[info.serviceName] = done + 1
            main.postDelayed({ if (!finished) resolve(info) }, RETRY_DELAY_MS)
        }

        @Suppress("DEPRECATION")
        private fun resolveNext() {
            if (finished || resolving) return
            val info = queue.removeFirstOrNull() ?: return
            resolving = true
            try {
                nsd.resolveService(info, object : NsdManager.ResolveListener {
                    override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
                        main.post { resolving = false; retry(info); resolveNext() }
                    }

                    override fun onServiceResolved(serviceInfo: NsdServiceInfo) {
                        main.post {
                            resolving = false
                            usable(serviceInfo.host, serviceInfo.port)?.let(::accept)
                            resolveNext()
                        }
                    }
                })
            } catch (_: Exception) {
                resolving = false
                resolveNext()
            }
        }

        private fun accept(address: String) {
            if (finished) return
            found += address
            finish(null)
        }

        /** Always releases the multicast lock, the discovery and the callbacks. */
        private fun finish(error: String?) {
            if (finished) return
            finished = true
            main.removeCallbacks(timeout)
            if (started) {
                try { nsd.stopServiceDiscovery(discovery) } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                callbacks.forEach { try { nsd.unregisterServiceInfoCallback(it) } catch (_: Exception) {} }
            }
            callbacks.clear()
            try { lock?.takeIf { it.isHeld }?.release() } catch (_: Exception) {}
            onClosed()
            if (error == null) result.success(found.toList()) else result.error(FAILED, error, null)
        }
    }

    companion object {
        const val CHANNEL = "dev.herdrnotify/discovery"
        private const val SERVICE_TYPE = "_excubia-pair._tcp"
        private const val PREFIX = "excubia-"
        private const val FAILED = "discovery_failed"
        private const val DEFAULT_TIMEOUT_MS = 15000L
        private const val MAX_RETRIES = 2
        private const val RETRY_DELAY_MS = 300L

        /** `ip:port` for a usable IPv4 address; IPv6, link-local and loopback are no use to the app. */
        private fun usable(host: InetAddress?, port: Int): String? =
            if (host is Inet4Address && !host.isLinkLocalAddress && !host.isLoopbackAddress && port > 0) {
                "${host.hostAddress}:$port"
            } else {
                null
            }
    }
}
