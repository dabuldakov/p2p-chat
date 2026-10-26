package com.p2pchat.app

import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiOutgoing
import uniffi.p2pchat_ffi.FfiOwnerElection

/** Минимальный порт отправки конвертов — удобно подменять в тестах. */
internal interface EnvelopeSender {
    fun sendTargets(envelope: ByteArray, targets: List<ByteArray>)
}

/** Операции владельца, которые нужны [OwnerRuntime]. */
internal interface OwnerCore {
    fun ownerAlive(group: ByteArray): FfiOutgoing?
    fun createOwnerClaim(group: ByteArray, trigger: UInt): FfiOutgoing?
    fun ownerVote(group: ByteArray, claimId: ByteArray, approve: Boolean, keep: Boolean): FfiOutgoing?
    fun ownerElection(group: ByteArray): FfiOwnerElection?
    fun finalizeOwnerElection(group: ByteArray): FfiOutgoing?
}

/** Адаптер Rust-ядра под [OwnerCore], чтобы координатор не зависел от ChatCore. */
internal class ChatCoreOwner(private val core: ChatCore) : OwnerCore {
    override fun ownerAlive(group: ByteArray) = core.ownerAlive(group)
    override fun createOwnerClaim(group: ByteArray, trigger: UInt) = core.createOwnerClaim(group, trigger)
    override fun ownerVote(group: ByteArray, claimId: ByteArray, approve: Boolean, keep: Boolean) =
        core.ownerVote(group, claimId, approve, keep)
    override fun ownerElection(group: ByteArray) = core.ownerElection(group)
    override fun finalizeOwnerElection(group: ByteArray) = core.finalizeOwnerElection(group)
}
