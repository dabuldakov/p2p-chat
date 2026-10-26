package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Test

class ReportSenderTest {

    @Test
    fun bodyCarriesBase64PayloadAndSignature() {
        val payload = byteArrayOf(1, 2, 3)
        val signature = byteArrayOf(9, 8, 7, 6)

        val json = ReportSender.body(payload, signature)

        assertEquals("""{"report":"AQID","signature":"CQgHBg=="}""", json)
    }
}
