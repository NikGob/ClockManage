package com.nikgob.clockmanage

import android.content.Context
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.os.Build
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/** Colours: Material You on Android 12+, the app's green otherwise; light and dark. */
class Palette(ctx: Context) {
    val dark = (ctx.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
    private fun sys(id: Int, fallback: String, c: Context) = if (Build.VERSION.SDK_INT >= 31) c.getColor(id) else Color.parseColor(fallback)

    val bg = if (dark) Color.parseColor("#111410") else Color.parseColor("#FCFDF7")
    val surface = if (dark) sys(android.R.color.system_neutral1_900, "#1D211B", ctx) else sys(android.R.color.system_neutral1_50, "#F0F5EC", ctx)
    val card = if (dark) Color.parseColor("#1D211B") else Color.parseColor("#EEF2E9")
    val text = if (dark) Color.parseColor("#E1E4DC") else Color.parseColor("#1A1C19")
    val muted = if (dark) Color.parseColor("#C3C8BB") else Color.parseColor("#43483F")
    val primary = if (dark) sys(android.R.color.system_accent1_200, "#8BD88A", ctx) else sys(android.R.color.system_accent1_600, "#2E7D32", ctx)
    val onPrimary = if (dark) sys(android.R.color.system_accent1_800, "#00390A", ctx) else Color.WHITE
    val container = if (dark) sys(android.R.color.system_accent1_700, "#0F5223", ctx) else sys(android.R.color.system_accent1_100, "#B6F2AF", ctx)
    val onContainer = if (dark) sys(android.R.color.system_accent1_100, "#B6F2AF", ctx) else sys(android.R.color.system_accent1_900, "#002204", ctx)
    val outline = if (dark) Color.parseColor("#43483F") else Color.parseColor("#C3C8BB")
    val error = if (dark) Color.parseColor("#FFB4AB") else Color.parseColor("#BA1A1A")
}

fun Context.dp(v: Float): Int = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics).toInt()
fun Context.dp(v: Int): Int = dp(v.toFloat())

fun rounded(color: Int, radius: Float, stroke: Int = 0, strokeColor: Int = 0) = GradientDrawable().apply {
    setColor(color)
    cornerRadius = radius
    if (stroke > 0) setStroke(stroke, strokeColor)
}

fun Context.label(text: String, sp: Float, color: Int, bold: Boolean = false): TextView = TextView(this).apply {
    this.text = text
    setTextSize(TypedValue.COMPLEX_UNIT_SP, sp)
    setTextColor(color)
    if (bold) typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
}

/** M3-ish pill button: filled, tonal or text. */
fun Context.pill(text: String, p: Palette, style: String = "filled", onClick: (View) -> Unit): Button = Button(this).apply {
    this.text = text
    isAllCaps = false
    stateListAnimator = null
    setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
    typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    val (bgc, fg) = when (style) {
        "filled" -> p.primary to p.onPrimary
        "tonal" -> p.container to p.onContainer
        else -> Color.TRANSPARENT to p.primary
    }
    setTextColor(fg)
    minHeight = dp(48)
    minimumHeight = dp(48)
    setPadding(dp(20), 0, dp(20), 0)
    background = RippleDrawable(ColorStateList.valueOf((fg and 0x00FFFFFF) or 0x33000000), rounded(bgc, dp(24).toFloat()), rounded(Color.WHITE, dp(24).toFloat()))
    setOnClickListener(onClick)
}

fun Context.card(p: Palette): LinearLayout = LinearLayout(this).apply {
    orientation = LinearLayout.VERTICAL
    background = rounded(p.card, dp(24).toFloat())
    setPadding(dp(20), dp(16), dp(20), dp(16))
}

fun ViewGroup.addWithMargins(v: View, top: Int = 0, bottom: Int = 0, width: Int = ViewGroup.LayoutParams.MATCH_PARENT): View {
    addView(v, LinearLayout.LayoutParams(width, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
        topMargin = top
        bottomMargin = bottom
    })
    return v
}

fun Context.row(): LinearLayout = LinearLayout(this).apply {
    orientation = LinearLayout.HORIZONTAL
    gravity = Gravity.CENTER_VERTICAL
}
