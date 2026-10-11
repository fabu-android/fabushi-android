package com.ombhrum.fabushi

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

internal typealias AppAlertAction = suspend () -> String?

internal data class AppAlertSecondaryAction(
    val label: String,
    val destructive: Boolean = false,
    val perform: AppAlertAction? = null,
)

internal data class AppAlertRequest(
    val title: String,
    val description: String? = null,
    val body: String? = null,
    val warning: String? = null,
    val confirmLabel: String,
    val pendingLabel: String? = null,
    val cancelLabel: String? = null,
    val destructive: Boolean = false,
    val secondary: AppAlertSecondaryAction? = null,
    val wide: Boolean = false,
    val perform: AppAlertAction? = null,
)

internal data class AppAlertState(
    val request: AppAlertRequest,
    val isPerforming: Boolean = false,
    val failure: String? = null,
)

internal class AppAlertController(
    private val scope: CoroutineScope,
) {
    private data class PendingAlert(
        val request: AppAlertRequest,
        val settle: (Boolean) -> Unit,
    )

    private val subscribers = linkedSetOf<() -> Unit>()
    private var active: PendingAlert? = null
    private var waiting: PendingAlert? = null
    private var snapshot: AppAlertState? = null
    private var epoch: Long = 0
    private var disposed = false
    private var actionJob: Job? = null

    fun getSnapshot(): AppAlertState? = snapshot

    fun subscribe(listener: () -> Unit): AutoCloseable {
        if (disposed) return AutoCloseable {}
        subscribers += listener
        return AutoCloseable { subscribers -= listener }
    }

    fun alert(request: AppAlertRequest, settle: (Boolean) -> Unit) {
        if (disposed) {
            settle(false)
            return
        }
        val entry = PendingAlert(request, settle)
        if (active == null) {
            show(entry)
            return
        }
        if (request.cancelLabel != null || waiting != null) {
            settle(false)
            return
        }
        waiting = entry
    }

    fun confirm() = performAction { it.perform }

    fun confirmSecondary() = performAction { it.secondary?.perform }

    fun cancel() {
        if (snapshot?.isPerforming == true) return
        settleActive(false)
    }

    fun reset() {
        epoch += 1
        actionJob?.cancel()
        actionJob = null
        val queued = waiting
        waiting = null
        queued?.settle?.invoke(false)
        settleActive(false, incrementEpoch = false)
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        epoch += 1
        actionJob?.cancel()
        actionJob = null
        val queued = waiting
        waiting = null
        queued?.settle?.invoke(false)
        settleActive(false, incrementEpoch = false)
        subscribers.clear()
    }

    internal fun subscriberCountForTest(): Int = subscribers.size

    private fun show(entry: PendingAlert) {
        active = entry
        publish(AppAlertState(request = entry.request))
    }

    private fun promoteWaiting() {
        if (disposed) return
        val next = waiting ?: return
        waiting = null
        show(next)
    }

    private fun settleActive(accepted: Boolean, incrementEpoch: Boolean = true) {
        val entry = active ?: return
        active = null
        if (incrementEpoch) epoch += 1
        actionJob = null
        publish(null)
        promoteWaiting()
        entry.settle(accepted)
    }

    private fun performAction(pick: (AppAlertRequest) -> AppAlertAction?) {
        val entry = active ?: return
        val current = snapshot ?: return
        if (current.isPerforming) return

        val action = pick(entry.request)
        if (action == null) {
            settleActive(true)
            return
        }

        val actionEpoch = epoch
        publish(current.copy(isPerforming = true, failure = null))
        actionJob = scope.launch {
            try {
                val result = action()
                if (actionEpoch != epoch || active !== entry || disposed) return@launch
                if (result == null) {
                    settleActive(true)
                } else {
                    snapshot?.let { publish(it.copy(isPerforming = false, failure = result)) }
                    actionJob = null
                }
            } catch (error: Throwable) {
                if (actionEpoch != epoch || active !== entry || disposed) return@launch
                val message = error.message ?: error.toString()
                snapshot?.let { publish(it.copy(isPerforming = false, failure = message)) }
                actionJob = null
            }
        }
    }

    private fun publish(next: AppAlertState?) {
        snapshot = next
        subscribers.toList().forEach { it() }
    }
}
