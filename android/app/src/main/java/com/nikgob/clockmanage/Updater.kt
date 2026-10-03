package com.nikgob.clockmanage

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.widget.Toast
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URI

/**
 * Self-update over Wi-Fi: the PC build carries the matching APK. On opening the app (at most
 * once an hour) the phone compares version codes; the user taps "Обновить", the APK is
 * downloaded from the PC and handed to the system installer (which asks for confirmation).
 */
object Updater {
    class Available(val code: Long, val name: String, val size: Long)

    private const val CHECK_EVERY_MS = 60 * 60_000L

    fun current(ctx: Context): Long {
        val info = ctx.packageManager.getPackageInfo(ctx.packageName, 0)
        return if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else @Suppress("DEPRECATION") info.versionCode.toLong()
    }

    fun currentName(ctx: Context): String = ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName ?: "?"

    /** Ask the PC (background thread). `null`: up to date, not paired, or no APK in this PC build. */
    fun check(ctx: Context, force: Boolean = false): Available? {
        val s = Store(ctx)
        val p = ctx.getSharedPreferences("upd", Context.MODE_PRIVATE)
        if (!s.paired) return null
        if (!force && System.currentTimeMillis() - p.getLong("checked", 0) < CHECK_EVERY_MS) {
            val code = p.getLong("code", 0)
            return if (code > current(ctx)) Available(code, p.getString("name", "") ?: "", p.getLong("size", 0)) else null
        }
        return try {
            val o = JSONObject(Api.of(s).get("/api/apk/info"))
            val code = o.optLong("code", 0)
            p.edit().putLong("checked", System.currentTimeMillis()).putLong("code", code)
                .putString("name", o.optString("name")).putLong("size", o.optLong("size")).apply()
            if (o.optBoolean("available") && code > current(ctx)) Available(code, o.optString("name"), o.optLong("size")) else null
        } catch (e: Exception) {
            null
        }
    }

    /** Installing needs "install unknown apps" for ClockManage, granted once in the settings. */
    fun canInstall(ctx: Context) = ctx.packageManager.canRequestPackageInstalls()

    fun openInstallPermission(ctx: Context) {
        ctx.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${ctx.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    /** Download the APK from the PC and start the system install (background thread). */
    fun downloadAndInstall(ctx: Context, progress: (Int) -> Unit): String? {
        val s = Store(ctx)
        val file = File(ctx.cacheDir, "update.apk")
        try {
            val c = URI("http://${s.host}:${s.port}/api/apk").toURL().openConnection() as HttpURLConnection
            c.connectTimeout = 4000
            c.readTimeout = 15000
            c.setRequestProperty("Authorization", "Bearer ${s.token}")
            if (c.responseCode != 200) return "ПК не отдал APK (${c.responseCode})"
            val total = c.contentLengthLong.coerceAtLeast(1)
            c.inputStream.use { inp ->
                file.outputStream().use { out ->
                    val buf = ByteArray(64 * 1024)
                    var done = 0L
                    var last = -1
                    while (true) {
                        val n = inp.read(buf)
                        if (n < 0) break
                        out.write(buf, 0, n)
                        done += n
                        val pct = (done * 100 / total).toInt()
                        if (pct != last) { last = pct; progress(pct) }
                    }
                }
            }
            c.disconnect()
        } catch (e: Exception) {
            return "Не скачалось: ${e.message ?: "нет связи с ПК"}"
        }
        return try {
            val installer = ctx.packageManager.packageInstaller
            val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
            params.setAppPackageName(ctx.packageName)
            val id = installer.createSession(params)
            installer.openSession(id).use { session ->
                session.openWrite("clockmanage", 0, file.length()).use { out ->
                    file.inputStream().use { it.copyTo(out) }
                    session.fsync(out)
                }
                val flags = PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
                val sender = PendingIntent.getBroadcast(ctx, id, Intent(ctx, InstallReceiver::class.java), flags)
                session.commit(sender.intentSender)
            }
            null
        } catch (e: Exception) {
            "Установка не началась: ${e.message}"
        }
    }
}

/** Results of the install session: show the system confirmation, or say what went wrong. */
class InstallReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                val confirm = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                else @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_INTENT)
                confirm?.let { ctx.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            }
            PackageInstaller.STATUS_SUCCESS -> Unit // the app restarts as the new version
            else -> {
                val msg = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "ошибка"
                Toast.makeText(ctx, "Обновление не установилось: $msg", Toast.LENGTH_LONG).show()
            }
        }
    }
}
