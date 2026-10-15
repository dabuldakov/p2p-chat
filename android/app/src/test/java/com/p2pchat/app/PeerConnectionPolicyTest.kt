package com.p2pchat.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PeerConnectionPolicyTest {

    private fun key(vararg head: Int): ByteArray {
        val a = ByteArray(32)
        head.forEachIndexed { i, v -> a[i] = v.toByte() }
        return a
    }

    private fun keyOf(hex: String): ByteArray =
        ByteArray(hex.length / 2) { hex.substring(it * 2, it * 2 + 2).toInt(16).toByte() }

    // Ключи из реального бага (b135… — этот телефон, 1443… — собеседник).
    private val me = keyOf("b135a09a18cfd4d746c1fcfdc07e85621ea5cf209fc403f2773bf004a683090e")
    private val peer = keyOf("1443b4ce21f6ed4f7d1a40290658a6f2e254fe31d3202bf08c9932fa4fa2546d")

    @Test
    fun ровно_один_инициатор_в_паре() {
        val pairs = listOf(
            me to peer,
            key(0x00) to key(0xff),
            key(0x01, 0x00) to key(0x00, 0x01),
            key(0x7f) to key(0x80),
        )
        for ((a, b) in pairs) {
            val aInit = PeerConnectionPolicy.weInitiate(a, b)
            val bInit = PeerConnectionPolicy.weInitiate(b, a)
            assertTrue("должен инициировать ровно один", aInit != bInit)
        }
    }

    @Test
    fun инициирует_меньший_ключ() {
        // b135… > 1443… → инициатор собеседник, не мы
        assertFalse(PeerConnectionPolicy.weInitiate(me, peer))
        assertTrue(PeerConnectionPolicy.weInitiate(peer, me))
    }

    @Test
    fun одинаковые_ключи_не_инициируют() {
        assertFalse(PeerConnectionPolicy.weInitiate(me, me))
        assertFalse(PeerConnectionPolicy.weInitiate(key(0x42), key(0x42)))
    }

    @Test
    fun хекс_без_разделителей_в_нижнем_регистре() {
        val hex = PeerConnectionPolicy.keyHex(byteArrayOf(0x00, 0x0a.toByte(), 0xff.toByte()))
        assertTrue(hex == "000aff")
    }

    @Test
    fun не_инициатор_никогда_не_шлёт_offer() {
        assertFalse(
            PeerConnectionPolicy.shouldOffer(
                isInitiator = false, dataChannelOpen = false,
                hasExisting = false, ageMs = 0, retryMs = 5_000,
            )
        )
    }

    @Test
    fun открытый_канал_не_пересоздаём() {
        assertFalse(
            PeerConnectionPolicy.shouldOffer(
                isInitiator = true, dataChannelOpen = true,
                hasExisting = true, ageMs = 60_000, retryMs = 5_000,
            )
        )
    }

    @Test
    fun инициатор_шлёт_offer_сразу_если_попытки_нет() {
        assertTrue(
            PeerConnectionPolicy.shouldOffer(
                isInitiator = true, dataChannelOpen = false,
                hasExisting = false, ageMs = 0, retryMs = 5_000,
            )
        )
    }

    @Test
    fun инициатор_не_дублирует_свежую_попытку() {
        assertFalse(
            PeerConnectionPolicy.shouldOffer(
                isInitiator = true, dataChannelOpen = false,
                hasExisting = true, ageMs = 4_999, retryMs = 5_000,
            )
        )
    }

    @Test
    fun инициатор_повторяет_зависшую_попытку() {
        assertTrue(
            PeerConnectionPolicy.shouldOffer(
                isInitiator = true, dataChannelOpen = false,
                hasExisting = true, ageMs = 5_000, retryMs = 5_000,
            )
        )
        assertTrue(
            PeerConnectionPolicy.shouldOffer(
                isInitiator = true, dataChannelOpen = false,
                hasExisting = true, ageMs = 15_000, retryMs = 5_000,
            )
        )
    }
}
