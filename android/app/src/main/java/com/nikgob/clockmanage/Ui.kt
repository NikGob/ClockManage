package com.nikgob.clockmanage

import android.content.Context
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.os.Build
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import org.json.JSONObject

/**
 * Colours: the PC's own scheme when it sent one (the app looks like the PC, light or dark as set
 * there), otherwise Material You on Android 12+ and the app's green; light and dark.
 */
class Palette(private val ctx: Context) {
    private val pc: JSONObject? = Store(ctx).theme?.let { runCatching { JSONObject(it) }.getOrNull() }
    private val systemDark = (ctx.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
    val dark = when (pc?.optString("mode")) {
        "dark" -> true
        "light" -> false
        else -> systemDark
    }
    private val roles = pc?.optJSONObject(if (dark) "dark" else "light")

    /** What the colours come from: the screen is built anew when it changes. */
    val key = "${pc?.toString()}|$dark"

    private fun sys(id: Int, fallback: String) = if (Build.VERSION.SDK_INT >= 31) ctx.getColor(id) else Color.parseColor(fallback)
    private fun role(name: String, fallback: Int): Int =
        roles?.optString(name)?.takeIf { it.startsWith("#") }?.let { runCatching { Color.parseColor(it) }.getOrNull() } ?: fallback

    val bg = role("surface", if (dark) Color.parseColor("#111410") else Color.parseColor("#FCFDF7"))
    val card = role("surface-container", if (dark) sys(android.R.color.system_neutral1_900, "#1D211B") else sys(android.R.color.system_neutral1_50, "#EEF2E9"))
    val cardHigh = role("surface-container-high", if (dark) sys(android.R.color.system_neutral1_800, "#272B25") else sys(android.R.color.system_neutral1_100, "#E6E9E0"))
    val cardHighest = role("surface-container-highest", if (dark) sys(android.R.color.system_neutral2_800, "#32362F") else sys(android.R.color.system_neutral2_100, "#E0E4DB"))
    val text = role("on-surface", if (dark) Color.parseColor("#E1E4DC") else Color.parseColor("#1A1C19"))
    val muted = role("on-surface-variant", if (dark) Color.parseColor("#C3C8BB") else Color.parseColor("#43483F"))
    val primary = role("primary", if (dark) sys(android.R.color.system_accent1_200, "#8BD88A") else sys(android.R.color.system_accent1_600, "#2E7D32"))
    val onPrimary = role("on-primary", if (dark) sys(android.R.color.system_accent1_800, "#00390A") else Color.WHITE)
    val container = role("primary-container", if (dark) sys(android.R.color.system_accent1_700, "#0F5223") else sys(android.R.color.system_accent1_100, "#B6F2AF"))
    val onContainer = role("on-primary-container", if (dark) sys(android.R.color.system_accent1_100, "#B6F2AF") else sys(android.R.color.system_accent1_900, "#002204"))
    val secondary = role("secondary-container", if (dark) sys(android.R.color.system_accent2_700, "#3A4B39") else sys(android.R.color.system_accent2_100, "#D3E8CF"))
    val onSecondary = role("on-secondary-container", if (dark) sys(android.R.color.system_accent2_100, "#D3E8CF") else sys(android.R.color.system_accent2_900, "#0F1F11"))
    val tertiary = role("tertiary-container", if (dark) sys(android.R.color.system_accent3_700, "#1F4D53") else sys(android.R.color.system_accent3_100, "#BCEBF1"))
    val onTertiary = role("on-tertiary-container", if (dark) sys(android.R.color.system_accent3_100, "#BCEBF1") else sys(android.R.color.system_accent3_900, "#001F23"))
    val outline = role("outline", if (dark) Color.parseColor("#8D9387") else Color.parseColor("#73796E"))
    val outlineVariant = role("outline-variant", if (dark) Color.parseColor("#43483F") else Color.parseColor("#C3C8BB"))
    val error = role("error", if (dark) Color.parseColor("#FFB4AB") else Color.parseColor("#BA1A1A"))
}

/** [c] at [alpha] (0..1). */
fun withAlpha(c: Int, alpha: Float): Int = (c and 0x00FFFFFF) or ((alpha * 255).toInt().coerceIn(0, 255) shl 24)

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

/** A Material icon (`R.drawable.ic_*`, the PC's set) in [color]. */
fun Context.iconView(res: Int, color: Int, size: Int = 24): ImageView = ImageView(this).apply {
    setImageResource(res)
    imageTintList = ColorStateList.valueOf(color)
    layoutParams = LinearLayout.LayoutParams(dp(size), dp(size))
}

/** M3 pill button: filled, tonal, outlined or text, with an optional leading icon. */
fun Context.pill(text: String, p: Palette, style: String = "filled", icon: Int = 0, onClick: (View) -> Unit): Button {
    val (bgc, fg) = when (style) {
        "filled" -> p.primary to p.onPrimary
        "tonal" -> p.secondary to p.onSecondary
        else -> Color.TRANSPARENT to p.primary
    }
    return pillIn(text, bgc, fg, if (style == "outlined") p.outline else 0, icon, onClick)
}

/** A pill in given colours (buttons on the coloured timer card), [stroke] for an outlined one. */
fun Context.pillIn(text: String, bgc: Int, fg: Int, stroke: Int = 0, icon: Int = 0, onClick: (View) -> Unit): Button = Button(this).apply {
    this.text = text
    isAllCaps = false
    stateListAnimator = null
    setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
    typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    setTextColor(fg)
    minHeight = dp(48)
    minimumHeight = dp(48)
    minWidth = 0
    minimumWidth = 0
    setPadding(dp(if (icon != 0) 16 else 20), 0, dp(20), 0)
    if (icon != 0) {
        setCompoundDrawablesRelativeWithIntrinsicBounds(icon, 0, 0, 0)
        compoundDrawablePadding = dp(8)
        compoundDrawableTintList = ColorStateList.valueOf(fg)
    }
    val r = dp(24).toFloat()
    background = RippleDrawable(ColorStateList.valueOf(withAlpha(fg, 0.2f)), rounded(bgc, r, if (stroke != 0) dp(1) else 0, stroke), rounded(Color.WHITE, r))
    setOnClickListener(onClick)
    pressBounce()
}

/** Press: sink a little; release: spring back with a small overshoot. Never eats the touch. */
@android.annotation.SuppressLint("ClickableViewAccessibility")
fun View.pressBounce() {
    setOnTouchListener { v, e ->
        when (e.actionMasked) {
            android.view.MotionEvent.ACTION_DOWN -> v.animate().scaleX(0.94f).scaleY(0.94f).setDuration(90)
                .setInterpolator(android.view.animation.DecelerateInterpolator()).start()
            android.view.MotionEvent.ACTION_UP, android.view.MotionEvent.ACTION_CANCEL -> v.animate().scaleX(1f).scaleY(1f).setDuration(380)
                .setInterpolator(android.view.animation.OvershootInterpolator(3f)).start()
        }
        false
    }
}

/** Children appear, disappear and resize smoothly (the system animates layout changes). */
fun ViewGroup.smoothChanges() {
    layoutTransition = android.animation.LayoutTransition().apply {
        enableTransitionType(android.animation.LayoutTransition.CHANGING)
        setDuration(260)
    }
}

/** A view arrives: rise and fade in, `delay` ms later (cascades on screen open). */
fun View.riseIn(delay: Long = 0) {
    alpha = 0f
    translationY = 24f * resources.displayMetrics.density
    animate().alpha(1f).translationY(0f).setStartDelay(delay).setDuration(420)
        .setInterpolator(android.view.animation.PathInterpolator(0.05f, 0.7f, 0.1f, 1f)).start()
}

/** A card like the PC's surfaces: rounded, on the surface container colour. */
fun Context.card(p: Palette): LinearLayout = LinearLayout(this).apply {
    orientation = LinearLayout.VERTICAL
    background = rounded(p.card, dp(28).toFloat())
    setPadding(dp(20), dp(18), dp(20), dp(18))
    smoothChanges()
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

/** The timer's ring, like the PC's dial: a faint track and the part already done. */
class Ring(ctx: Context) : View(ctx) {
    private val stroke = ctx.dp(10).toFloat()
    private val track = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeWidth = stroke }
    private val arc = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeWidth = stroke; strokeCap = Paint.Cap.ROUND }
    private val box = RectF()

    /** 0..1 of the circle; below 0 hides the arc. */
    var fraction = -1f
        set(v) { if (field != v) { field = v; invalidate() } }

    fun colors(arcColor: Int, trackColor: Int) {
        arc.color = arcColor
        track.color = trackColor
        invalidate()
    }

    override fun onDraw(c: Canvas) {
        val half = stroke / 2
        box.set(half, half, width - half, height - half)
        c.drawOval(box, track)
        if (fraction > 0f) c.drawArc(box, -90f, 360f * fraction.coerceAtMost(1f), false, arc)
    }
}

/** A slim rounded progress bar (0..1000), like the PC's plan rows. */
class Bar(ctx: Context) : View(ctx) {
    private val back = Paint(Paint.ANTI_ALIAS_FLAG)
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val box = RectF()

    /** 0..1000; animated with ObjectAnimator.ofInt(bar, "progress", …). */
    var progress = 0
        set(v) { if (field != v) { field = v.coerceIn(0, 1000); invalidate() } }

    fun colors(fillColor: Int, backColor: Int) {
        fill.color = fillColor
        back.color = backColor
        invalidate()
    }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(MeasureSpec.getSize(w), context.dp(4))

    override fun onDraw(c: Canvas) {
        val r = height / 2f
        box.set(0f, 0f, width.toFloat(), height.toFloat())
        c.drawRoundRect(box, r, r, back)
        if (progress > 0) {
            box.right = (width * progress / 1000f).coerceAtLeast(height.toFloat())
            c.drawRoundRect(box, r, r, fill)
        }
    }
}
