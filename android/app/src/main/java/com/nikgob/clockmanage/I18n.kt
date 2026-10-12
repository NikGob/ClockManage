package com.nikgob.clockmanage

import java.util.Locale
import org.json.JSONObject

/**
 * Texts: the same flat JSON files as the PC (`ui/locales/<lang>.json`, packed into the APK as
 * Java resources by build.gradle.kts). The language is the phone's own — Russian for ru/uk/be,
 * English otherwise: tr("android_main_find_pc"), tr("android_main_paired_with", "name" to pc).
 */
object I18n {
    val ru: Boolean get() = Locale.getDefault().language in setOf("ru", "uk", "be")

    private fun load(name: String): Map<String, String> = runCatching {
        val text = I18n::class.java.getResourceAsStream("/$name.json")!!.bufferedReader().use { it.readText() }
        val o = JSONObject(text)
        o.keys().asSequence().associateWith { o.getString(it) }
    }.getOrDefault(emptyMap())

    private val en by lazy { load("en") }
    private val ruTexts by lazy { load("ru") }

    fun text(key: String): String = (if (ru) ruTexts[key] else null) ?: en[key] ?: key
}

/** The text of `key` with its `{name}` arguments filled in. */
fun tr(key: String, vararg args: Pair<String, Any?>): String {
    val text = I18n.text(key)
    if (args.isEmpty()) return text
    return Regex("""\{(\w+)}""").replace(text) { m -> args.firstOrNull { it.first == m.groupValues[1] }?.second?.toString() ?: m.value }
}
