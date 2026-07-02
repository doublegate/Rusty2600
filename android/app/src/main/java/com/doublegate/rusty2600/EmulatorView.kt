package com.doublegate.rusty2600

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.util.AttributeSet
import android.view.View
import java.nio.ByteBuffer

/**
 * Renders the 160x192 RGBA8 framebuffer `MobileEmulator.runFrame` produces.
 *
 * Nearest-neighbor scaled to fill the view (`isFilterBitmap = false`),
 * matching the native/wasm frontends' pixelated look rather than a smoothed
 * blow-up. `Bitmap.Config.ARGB_8888`'s in-memory byte order is R,G,B,A —
 * the same layout `MobileEmulator.runFrame`'s `rgba` output already uses, so
 * `copyPixelsFromBuffer` needs no channel reordering.
 */
class EmulatorView
@JvmOverloads
constructor(context: Context, attrs: AttributeSet? = null) : View(context, attrs) {

    private val bitmap = Bitmap.createBitmap(FRAME_W, FRAME_H, Bitmap.Config.ARGB_8888)
    private val paint = Paint().apply { isFilterBitmap = false }
    private val srcRect = Rect(0, 0, FRAME_W, FRAME_H)

    /** Blit this frame's RGBA8 bytes and request a redraw. */
    fun updateFrame(rgba: ByteArray) {
        bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(rgba))
        postInvalidate()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        canvas.drawBitmap(bitmap, srcRect, Rect(0, 0, width, height), paint)
    }

    companion object {
        /** Matches `rusty2600_mobile::ATARI_W`. */
        const val FRAME_W = 160

        /** Matches `rusty2600_mobile::ATARI_H`. */
        const val FRAME_H = 192
    }
}
