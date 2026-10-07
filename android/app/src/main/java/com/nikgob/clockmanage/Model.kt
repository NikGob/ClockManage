package com.nikgob.clockmanage

import org.json.JSONArray
import org.json.JSONObject

/** One phase of the day the PC projected forward ("nobody presses anything"). */
class Entry(
    val from: Long,
    val until: Long?,
    val kind: String,
    val title: String,
    val subtitle: String,
    val block: Int?,
    val paused: Boolean,
    val elapsedMs: Long,
    val durMs: Long,
    val blocksWork: LongArray,
    /** A segment whose end is a loud alarm (a nap). Never set while [prep]. */
    val alarm: Boolean,
    /** A nap still getting ready: [segmentEnd] is the end of the preparation, a reminder to lie
     *  down; nothing counts down until "Лёг". */
    val prep: Boolean,
    /** A segment "без времени": a stopwatch from [from], no end and no reminders. */
    val stopwatch: Boolean,
) {
    val running get() = until != null && !paused
    val segment get() = kind == "segment"
    /** Planned end of a segment (it then runs over until the user ends it). */
    val segmentEnd get() = from + durMs
    /** A segment that reminds at [segmentEnd]: not a stopwatch. */
    val segmentTimed get() = segment && !stopwatch

    fun remaining(now: Long): Long = when {
        paused -> (durMs - elapsedMs).coerceAtLeast(0)
        until != null -> (until - now).coerceAtLeast(0)
        segment -> segmentEnd - now // negative = over time
        else -> 0
    }

    fun elapsed(now: Long): Long = when {
        paused -> elapsedMs
        until != null -> (now - from).coerceIn(0, durMs)
        else -> (now - from).coerceAtLeast(0)
    }

    /** Work of block [i] at [now]: the running block grows while its part runs. */
    fun work(i: Int, now: Long): Long {
        val base = blocksWork.getOrElse(i) { 0 }
        return if (kind == "work" && block == i) base + elapsed(now) else base
    }
}

class Block(
    val name: String,
    val minutes: Int,
    val done: Boolean,
    val started: Boolean,
    val parts: Int,
    val partsDone: Int,
    val minMinutes: Int,
    val isBreak: Boolean,
)

class Snapshot(
    val version: String,
    val serverNow: Long,
    /** How long the PC held a long poll before stamping [serverNow] (-1: an older PC didn't say). */
    val heldMs: Long,
    /** Version code of the APK the PC carries (-1: an older PC didn't say). */
    val apkCode: Long,
    val started: Boolean,
    val completed: Boolean,
    val studyDay: Boolean,
    val mode: String,
    val dayEnd: String,
    val dayEndAt: Long,
    val lockBase: Boolean,
    val lockBlocked: Boolean,
    val lockReason: String,
    val lockUntil: Long?,
    val doneAt: Long?,
    val blocks: List<Block>,
    val timeline: List<Entry>,
    val canStartDay: Boolean,
    val canPause: Boolean,
    val canResume: Boolean,
    val canStartNext: Boolean,
    val canEditPlan: Boolean,
    val canEndSegment: Boolean,
    val sites: List<String>,
    val apps: Set<String>,
) {
    /** The phase at [now] (PC clock): the first entry that has not ended yet. */
    fun entryAt(now: Long): Entry? = timeline.firstOrNull { it.until == null || now < it.until } ?: timeline.lastOrNull()

    /**
     * Should the phone block right now? Works offline from the last snapshot: the lock lasts
     * until the day end, an access window ends, or the plan closes by itself.
     * A snapshot older than 20 hours is not trusted at all.
     */
    fun blockedAt(now: Long, syncedAt: Long): Boolean {
        if (now - syncedAt > 20 * HOUR) return false
        if (!lockBase || now >= dayEndAt) return false
        if (doneAt != null && now >= doneAt) return false
        if (!lockBlocked) return lockUntil != null && now >= lockUntil
        return true
    }

    companion object {
        const val MIN = 60_000L
        const val HOUR = 60 * MIN

        private fun JSONObject.longOrNull(k: String): Long? = if (isNull(k) || !has(k)) null else getLong(k)
        private fun JSONObject.intOrNull(k: String): Int? = if (isNull(k) || !has(k)) null else getInt(k)
        private fun JSONArray.strings(): List<String> = (0 until length()).map { getString(it) }

        fun parse(json: String): Snapshot {
            val o = JSONObject(json)
            val lock = o.getJSONObject("lock")
            val can = o.getJSONObject("can")
            val blocks = o.getJSONArray("blocks").let { a ->
                (0 until a.length()).map { i ->
                    val b = a.getJSONObject(i)
                    Block(
                        b.getString("name"), b.getInt("minutes"), b.getBoolean("done"), b.getBoolean("started"),
                        b.getInt("parts"), b.getInt("parts_done"), b.getInt("min_minutes"),
                        b.optString("kind", "study") == "break",
                    )
                }
            }
            val timeline = o.getJSONArray("timeline").let { a ->
                (0 until a.length()).map { i ->
                    val e = a.getJSONObject(i)
                    val w = e.getJSONArray("blocks_work_ms")
                    Entry(
                        e.getLong("from"), e.longOrNull("until"), e.getString("kind"), e.getString("title"),
                        e.getString("subtitle"), e.intOrNull("block"), e.getBoolean("paused"),
                        e.getLong("elapsed_ms"), e.getLong("dur_ms"), LongArray(w.length()) { w.getLong(it) },
                        e.optBoolean("alarm", false),
                        e.optBoolean("prep", false),
                        e.optBoolean("stopwatch", false),
                    )
                }
            }
            return Snapshot(
                version = o.getString("version"),
                serverNow = o.getLong("server_now"),
                heldMs = o.optLong("held_ms", -1),
                apkCode = o.optLong("apk_code", -1),
                started = o.getBoolean("started"),
                completed = o.getBoolean("completed"),
                studyDay = o.getBoolean("study_day"),
                mode = o.getString("mode"),
                dayEnd = o.getString("day_end"),
                dayEndAt = o.getLong("day_end_at"),
                lockBase = lock.getBoolean("base"),
                lockBlocked = lock.getBoolean("blocked"),
                lockReason = lock.getString("reason"),
                lockUntil = lock.longOrNull("until"),
                doneAt = o.longOrNull("done_at"),
                blocks = blocks,
                timeline = timeline,
                canStartDay = can.getBoolean("start_day"),
                canPause = can.getBoolean("pause"),
                canResume = can.getBoolean("resume"),
                canStartNext = can.getBoolean("start_next"),
                canEditPlan = can.getBoolean("edit_plan"),
                canEndSegment = can.optBoolean("end_segment", false),
                sites = o.getJSONArray("sites").strings(),
                apps = o.getJSONArray("apps").strings().toSet(),
            )
        }
    }
}

object Fmt {
    fun mmss(ms: Long): String {
        val s = (ms.coerceAtLeast(0) + 999) / 1000
        return if (s >= 3600) "%d:%02d:%02d".format(s / 3600, s / 60 % 60, s % 60) else "%02d:%02d".format(s / 60, s % 60)
    }

    fun dur(ms: Long): String {
        val m = (ms.coerceAtLeast(0) + 30_000) / 60_000
        val h = m / 60
        val r = m % 60
        return when {
            h == 0L -> "$r мин"
            r == 0L -> "$h ч"
            else -> "$h ч $r мин"
        }
    }

    fun hours(min: Int): String = when {
        min % 60 == 0 -> "${min / 60} ч"
        min < 60 -> "$min мин"
        else -> "${min / 60} ч ${min % 60} мин"
    }
}
