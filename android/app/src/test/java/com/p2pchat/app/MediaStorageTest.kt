package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.p2pchat_ffi.FfiAttachment

class MediaStorageTest {

    @Test
    fun categoriesMapToMediaKinds() {
        assertEquals(setOf("image"), MediaStorage.skipKinds("photos"))
        assertEquals(setOf("video", "video_note"), MediaStorage.skipKinds("videos"))
        assertEquals(setOf("audio", "voice"), MediaStorage.skipKinds("audio"))
        assertEquals(5, MediaStorage.skipKinds("all")?.size)
        assertNull(MediaStorage.skipKinds("temp"))
        assertNull(MediaStorage.skipKinds("gifs"))
    }

    @Test
    fun skipsOnlyAttachmentsOfClearedKinds() {
        val attachments = listOf(
            attachment("img", "image"),
            attachment("vid", "video"),
            attachment("note", "video_note"),
            attachment("voice", "voice"),
            attachment("gif", "klipy"),
        )

        assertEquals(listOf("img"), MediaStorage.attachmentsToSkip("photos", attachments))
        assertEquals(listOf("vid", "note"), MediaStorage.attachmentsToSkip("videos", attachments))
        assertEquals(listOf("voice"), MediaStorage.attachmentsToSkip("audio", attachments))
        assertEquals(4, MediaStorage.attachmentsToSkip("all", attachments).size)
        assertEquals(emptyList<String>(), MediaStorage.attachmentsToSkip("links", attachments))
    }

    private fun attachment(id: String, kind: String) = FfiAttachment(
        kind = kind,
        id = id,
        width = 0u,
        height = 0u,
        url = "",
        mime = "",
        size = 0uL,
        durationMs = 0uL,
        fileName = "",
        thumbnail = ByteArray(0),
    )
}
