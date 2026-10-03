package com.nikgob.clockmanage

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import org.json.JSONObject
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.SocketTimeoutException
import java.net.URI

/** Everything the app remembers: the paired PC and the last snapshot. */
class Store(ctx: Context) {
    private val p: SharedPreferences = ctx.applicationContext.getSharedPreferences("cm", Context.MODE_PRIVATE)

    var host: String get() = p.getString("host", "") ?: ""; set(v) = p.edit().putString("host", v).apply()
    var port: Int get() = p.getInt("port", 47811); set(v) = p.edit().putInt("port", v).apply()
    var token: String get() = p.getString("token", "") ?: ""; set(v) = p.edit().putString("token", v).apply()
    var pcName: String get() = p.getString("pc", "") ?: ""; set(v) = p.edit().putString("pc", v).apply()
    var notify: Boolean get() = p.getBoolean("notify", true); set(v) = p.edit().putBoolean("notify", v).apply()

    /** PC clock minus phone clock, measured on every sync. */
    var offset: Long get() = p.getLong("offset", 0); private set(v) = p.edit().putLong("offset", v).apply()
    /** PC time of the last successful sync. */
    var syncedAt: Long get() = p.getLong("synced", 0); private set(v) = p.edit().putLong("synced", v).apply()
    /** Phone time of the last attempt (success or not), to rate-limit background syncs. */
    var triedAt: Long get() = p.getLong("tried", 0); set(v) = p.edit().putLong("tried", v).apply()

    val paired get() = host.isNotEmpty() && token.isNotEmpty()

    /** Now on the PC clock. */
    fun now(): Long = System.currentTimeMillis() + offset

    fun snapshot(): Snapshot? {
        val raw = p.getString("snap", null) ?: return null
        return try { Snapshot.parse(raw) } catch (e: Exception) { null }
    }

    fun save(raw: String, snap: Snapshot, sentAt: Long, gotAt: Long) {
        // The PC stamped server_now somewhere between sending and receiving: take the middle.
        val off = snap.serverNow - (sentAt + gotAt) / 2
        p.edit().putString("snap", raw).putLong("offset", off).putLong("synced", snap.serverNow).apply()
    }

    fun forget() {
        p.edit().remove("token").remove("snap").remove("synced").apply()
    }
}

class ApiError(message: String, val unauthorized: Boolean = false) : IOException(message)

/** Tiny HTTP client for the PC API (no libraries: HttpURLConnection + org.json). */
class Api(private val host: String, private val port: Int, private val token: String) {

    fun get(path: String, readTimeoutMs: Int = 8000): String = request("GET", path, null, readTimeoutMs)

    fun post(path: String, body: JSONObject): String = request("POST", path, body.toString(), 8000)

    private fun request(method: String, path: String, body: String?, readTimeoutMs: Int): String {
        val c = URI("http://$host:$port$path").toURL().openConnection() as HttpURLConnection
        try {
            c.requestMethod = method
            c.connectTimeout = 3000
            c.readTimeout = readTimeoutMs
            c.useCaches = false
            if (token.isNotEmpty()) c.setRequestProperty("Authorization", "Bearer $token")
            if (body != null) {
                c.doOutput = true
                c.setRequestProperty("Content-Type", "application/json; charset=utf-8")
                c.outputStream.use { it.write(body.toByteArray()) }
            }
            val code = c.responseCode
            val text = (if (code < 400) c.inputStream else c.errorStream)?.bufferedReader()?.use { it.readText() } ?: ""
            if (code >= 400) {
                val msg = try { JSONObject(text).getString("error") } catch (e: Exception) { "Ошибка $code" }
                throw ApiError(msg, unauthorized = code == 401)
            }
            return text
        } catch (e: SocketTimeoutException) {
            throw ApiError("ПК не отвечает")
        } catch (e: ApiError) {
            throw e
        } catch (e: IOException) {
            throw ApiError("Нет связи с ПК")
        } finally {
            c.disconnect()
        }
    }

    companion object {
        fun of(s: Store) = Api(s.host, s.port, s.token)

        /** Exchange the PIN shown on the PC for a token. Returns (token, pc name). */
        fun pair(host: String, port: Int, pin: String): Pair<String, String> {
            val name = "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}".take(40)
            val r = JSONObject(Api(host, port, "").post("/api/pair", JSONObject().put("pin", pin).put("name", name)))
            return r.getString("token") to r.optString("pc_name", host)
        }
    }
}

/** A PC that answered the discovery broadcast. */
class FoundPc(val name: String, val host: String, val port: Int)

object Discovery {
    /** Broadcast `CLOCKMANAGE_DISCOVER` on the Wi-Fi and collect answers for [ms]. */
    fun find(ms: Int = 1500): List<FoundPc> {
        val out = linkedMapOf<String, FoundPc>()
        try {
            DatagramSocket().use { sock ->
                sock.broadcast = true
                sock.soTimeout = 300
                val msg = "CLOCKMANAGE_DISCOVER".toByteArray()
                val deadline = System.currentTimeMillis() + ms
                var lastSend = 0L
                val buf = ByteArray(512)
                while (System.currentTimeMillis() < deadline) {
                    if (System.currentTimeMillis() - lastSend > 500) {
                        lastSend = System.currentTimeMillis()
                        sock.send(DatagramPacket(msg, msg.size, InetAddress.getByName("255.255.255.255"), 47810))
                    }
                    try {
                        val p = DatagramPacket(buf, buf.size)
                        sock.receive(p)
                        val o = JSONObject(String(p.data, 0, p.length))
                        if (o.optString("app") == "clockmanage") {
                            val host = p.address.hostAddress ?: continue
                            out[host] = FoundPc(o.optString("name", host), host, o.optInt("port", 47811))
                        }
                    } catch (e: SocketTimeoutException) {
                        // keep listening until the deadline
                    } catch (e: org.json.JSONException) {
                        // not ours
                    }
                }
            }
        } catch (e: IOException) {
            // no network
        }
        return out.values.toList()
    }
}
