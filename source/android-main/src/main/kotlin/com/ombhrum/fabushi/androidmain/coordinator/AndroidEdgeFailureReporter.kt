package com.ombhrum.fabushi.androidmain.coordinator

internal data class AndroidEdgeFailure(
    val area: String,
    val leg: String,
    val errorClass: String,
)

internal class AndroidEdgeFailureBuffer(
    private val preInstallCapacity: Int = 32,
) {
    private var reporter: ((AndroidEdgeFailure) -> Unit)? = null
    private val pending = ArrayDeque<AndroidEdgeFailure>()

    @Synchronized
    fun install(next: ((AndroidEdgeFailure) -> Unit)?) {
        reporter = next
        val flush = pending.toList()
        pending.clear()
        if (next != null) flush.forEach(next)
    }

    @Synchronized
    fun report(failure: AndroidEdgeFailure) {
        reporter?.let {
            it(failure)
            return
        }
        if (pending.size < preInstallCapacity) pending.addLast(failure)
    }
}

internal object AndroidEdgeFailures {
    private val owner = AndroidEdgeFailureBuffer()

    fun installReporter(reporter: ((AndroidEdgeFailure) -> Unit)?) = owner.install(reporter)

    fun report(area: String, leg: String, error: Throwable) =
        reportClass(area, leg, error::class.java.simpleName.ifBlank { "Throwable" })

    fun reportClass(area: String, leg: String, errorClass: String) {
        owner.report(
            AndroidEdgeFailure(
                area = area,
                leg = leg,
                errorClass = errorClass.ifBlank { "Error" },
            ),
        )
    }
}
