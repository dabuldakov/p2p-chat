package com.p2pchat.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MediaTransferTest {

    @Test
    fun groupMembershipComparesBytesByContent() {
        val group = byteArrayOf(1, 2, 3)

        assertTrue(groupListContains(listOf(byteArrayOf(1, 2, 3)), group))
        assertTrue(groupListContains(listOf(byteArrayOf(9), byteArrayOf(1, 2, 3)), group))
        assertFalse(groupListContains(emptyList(), group))
        assertFalse(groupListContains(listOf(byteArrayOf(3, 2, 1)), group))
    }
}
