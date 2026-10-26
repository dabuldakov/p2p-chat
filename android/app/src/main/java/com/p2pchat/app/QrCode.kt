package com.p2pchat.app

import androidx.core.graphics.createBitmap
import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel

/**
 * Генерирует QR-код с публичным ключом узла — чтобы собеседник мог
 * отсканировать его вместо ручного ввода hex.
 */
fun generateQrBitmap(text: String, size: Int = 640): ImageBitmap {
    val hints = mapOf(
        EncodeHintType.MARGIN to 1,
        EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M,
        EncodeHintType.CHARACTER_SET to "UTF-8",
    )
    val matrix = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size, hints)

    val pixels = IntArray(size * size)
    var i = 0
    for (y in 0 until size) {
        for (x in 0 until size) {
            pixels[i++] = if (matrix[x, y]) Color.BLACK else Color.WHITE
        }
    }
    val bmp = createBitmap(size, size)
    bmp.setPixels(pixels, 0, size, 0, 0, size, size)
    return bmp.asImageBitmap()
}
