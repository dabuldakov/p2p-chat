package com.p2pchat.app

import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.p2pchat_ffi.FfiOutgoing
import uniffi.p2pchat_ffi.FfiOwnerElection

class OwnerRuntimeTest {

    private val group = byteArrayOf(1, 2, 3)
    private val out = FfiOutgoing(byteArrayOf(9), emptyList())

    private class FakeCore : OwnerCore {
        var election: FfiOwnerElection? = null
        var claim: FfiOutgoing? = null
        var vote: FfiOutgoing? = null
        var alive: FfiOutgoing? = null
        var finalize: FfiOutgoing? = null
        var finalized = 0

        override fun ownerAlive(group: ByteArray) = alive
        override fun createOwnerClaim(group: ByteArray, trigger: UInt) = claim
        override fun ownerVote(group: ByteArray, claimId: ByteArray, approve: Boolean, keep: Boolean) = vote
        override fun ownerElection(group: ByteArray) = election
        override fun finalizeOwnerElection(group: ByteArray): FfiOutgoing? {
            finalized++
            return finalize
        }
    }

    private class FakeSender : EnvelopeSender {
        var sent = 0
        override fun sendTargets(envelope: ByteArray, targets: List<ByteArray>) { sent++ }
    }

    private fun election(endsAt: ULong) = FfiOwnerElection(
        trigger = 1u,
        createdAt = 0uL,
        voteStart = 0uL,
        endsAt = endsAt,
        claims = emptyList(),
        votes = emptyList(),
    )

    @Test
    fun heartbeatOnlyOncePerWindow() = runTest {
        val dispatcher = UnconfinedTestDispatcher(testScheduler)
        val core = FakeCore().apply { alive = out }
        val sender = FakeSender()
        val runtime = OwnerRuntime(core, sender, this, dispatcher, { group }, { true }, { 0L }, { null }, { 0 })

        runtime.heartbeat()
        runCurrent()
        runtime.heartbeat()
        runCurrent()
        assertEquals(1, sender.sent)
    }

    @Test
    fun advanceFinalizesExistingElectionWithoutWaitingForDeadline() = runTest {
        val dispatcher = UnconfinedTestDispatcher(testScheduler)
        val core = FakeCore().apply {
            election = election(ULong.MAX_VALUE)
            finalize = out
        }
        val sender = FakeSender()
        val runtime = OwnerRuntime(core, sender, this, dispatcher, { group }, { true }, { 0L }, { core.election }, { 0 })

        runtime.advance()
        runCurrent()
        assertEquals(1, core.finalized)
        assertEquals(1, sender.sent)
    }

    @Test
    fun rejectedClaimExplainsActiveElection() = runTest {
        val dispatcher = UnconfinedTestDispatcher(testScheduler)
        val core = FakeCore().apply { election = election(1uL) }
        val toasts = mutableListOf<Int>()
        val runtime = OwnerRuntime(core, FakeSender(), this, dispatcher, { group }, { false }, { 0L }, { core.election }, { 2 }) {
            toasts += it
        }

        runtime.claim(1)
        runCurrent()
        assertEquals(listOf(R.string.owner_election_title), toasts)
    }
}
