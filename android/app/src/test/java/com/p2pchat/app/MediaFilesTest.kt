package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Test

class MediaFilesTest {
    @Test
    fun mime_extensions() {
        assertEquals("jpg", MediaFiles.extension("image/jpeg"))
        assertEquals("mp4", MediaFiles.extension("video/mp4"))
        assertEquals("m4a", MediaFiles.extension("audio/mp4"))
        assertEquals("mp3", MediaFiles.extension("audio/mpeg"))
        assertEquals("bin", MediaFiles.extension("application/octet-stream"))
    }

    @Test
    fun categorizes_media_files() {
        assertEquals(MediaCategory.PHOTOS, MediaFiles.categoryOf(java.io.File("a.jpg")))
        assertEquals(MediaCategory.PHOTOS, MediaFiles.categoryOf(java.io.File("a.webp")))
        assertEquals(MediaCategory.VIDEOS, MediaFiles.categoryOf(java.io.File("a.mp4")))
        assertEquals(MediaCategory.AUDIO, MediaFiles.categoryOf(java.io.File("a.m4a")))
        assertEquals(MediaCategory.TEMP, MediaFiles.categoryOf(java.io.File("abc.part")))
        assertEquals(MediaCategory.TEMP, MediaFiles.categoryOf(java.io.File("media-1.mp4")))
        assertEquals(MediaCategory.OTHER, MediaFiles.categoryOf(java.io.File("a.bin")))
    }

    @Test
    fun formats_sizes() {
        assertEquals("0 B", formatBytes(0))
        assertEquals("1 KB", formatBytes(1024))
        assertEquals("1.0 MB", formatBytes(1024 * 1024))
    }

    @Test
    fun sha256_is_hex_and_stable() {
        assertEquals(
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            MediaFiles.sha256("hello".toByteArray()),
        )
    }
}
