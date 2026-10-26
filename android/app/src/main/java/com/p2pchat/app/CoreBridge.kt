package com.p2pchat.app

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiOutgoing

/** Асинхронные вызовы Rust-ядра и приём входящих конвертов. */
internal class CoreBridge(
    private val core: ChatCore,
    private val sender: EnvelopeSender,
    private val scope: CoroutineScope,
    private val onDone: () -> Unit,
    private val onReceived: () -> Unit,
) {
    fun out(block: (ChatCore) -> FfiOutgoing?) {
        scope.launch {
            val out = withContext(Dispatchers.IO) {
                runCatching { block(core) }.onFailure { Logs.w("core action: $it") }.getOrNull()
            } ?: return@launch
            sender.sendTargets(out.envelope, out.targets)
            onDone()
        }
    }

    fun outs(block: (ChatCore) -> List<FfiOutgoing>) {
        scope.launch {
            val outs = withContext(Dispatchers.IO) {
                runCatching { block(core) }.onFailure { Logs.w("core action: $it") }.getOrNull()
            } ?: return@launch
            outs.forEach { sender.sendTargets(it.envelope, it.targets) }
            onDone()
        }
    }

    fun receive(envelope: ByteArray) {
        scope.launch {
            val outs = withContext(Dispatchers.IO) {
                runCatching { core.receive(envelope) }.getOrElse {
                    Logs.w("receive error: $it")
                    null
                }
            } ?: return@launch
            Logs.d("incoming env -> out=${outs.size}")
            outs.forEach { out -> runCatching { sender.sendTargets(out.envelope, out.targets) } }
            onDone()
            runCatching { onReceived() }
        }
    }
}
