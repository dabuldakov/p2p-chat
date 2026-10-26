package com.p2pchat.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.p2pchat_ffi.FfiAttachment

class ChatSnapshotTest {

    @Test
    fun groupAndByteEqualityHandlesNullAndContent() {
        assertTrue(sameGroup(null, null))
        assertFalse(sameGroup(null, byteArrayOf(1)))
        assertTrue(sameGroup(byteArrayOf(1, 2), byteArrayOf(1, 2)))
        assertFalse(sameGroup(byteArrayOf(1, 2), byteArrayOf(2, 1)))
        assertTrue(sameBytes(null, null))
        assertFalse(sameBytes(byteArrayOf(1), null))
    }

    @Test
    fun avatarMapsCompareAllContent() {
        val a = mapOf("one" to byteArrayOf(1), "two" to byteArrayOf(2))
        val b = mapOf("one" to byteArrayOf(1), "two" to byteArrayOf(2))
        assertTrue(sameAvatars(a, b))
        assertFalse(sameAvatars(a, mapOf("one" to byteArrayOf(1))))
        assertFalse(sameAvatars(a, mapOf("one" to byteArrayOf(1), "two" to byteArrayOf(3))))
    }

    @Test
    fun gifRefDropsMissingDimensions() {
        val ref = toGifRef(attachment(width = 320u, height = 0u))
        org.junit.Assert.assertEquals(320, ref.width)
        assertNull(ref.height)
        org.junit.Assert.assertEquals("gif-id", ref.providerId)
    }

    private fun attachment(width: UInt, height: UInt) = FfiAttachment(
        kind = "gif",
        id = "gif-id",
        width = width,
        height = height,
        url = "",
        mime = "image/gif",
        size = 0uL,
        durationMs = 0uL,
        fileName = "",
        thumbnail = ByteArray(0),
    )
}
