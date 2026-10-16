package com.ombhrum.fabushi

internal class LocalToolPermissionScopeGate {
    private var activeScope: String? = null
    private var activeRevision: Long? = null
    private var reentryFloor: Long? = null
    private var disposed = false
    private val highestRevisionByScope = mutableMapOf<String, Long>()

    fun enter(scope: String?) {
        if (disposed || activeScope == scope) return
        val previousScope = activeScope
        val previousRevision = activeRevision
        if (previousScope != null && previousRevision != null) {
            highestRevisionByScope[previousScope] =
                maxOf(highestRevisionByScope[previousScope] ?: Long.MIN_VALUE, previousRevision)
        }
        activeScope = scope
        activeRevision = null
        reentryFloor = scope?.let(highestRevisionByScope::get)
    }

    fun accepts(scope: String, revision: Long): Boolean {
        if (disposed || activeScope != scope || revision < 0) return false
        val current = activeRevision
        if (current == null) {
            if (reentryFloor?.let { revision <= it } == true) return false
            activeRevision = revision
            reentryFloor = null
            highestRevisionByScope[scope] =
                maxOf(highestRevisionByScope[scope] ?: Long.MIN_VALUE, revision)
            return true
        }
        if (revision < current) return false
        if (revision > current) {
            activeRevision = revision
            highestRevisionByScope[scope] =
                maxOf(highestRevisionByScope[scope] ?: Long.MIN_VALUE, revision)
        }
        return true
    }

    fun reset() {
        if (disposed) return
        activeScope = null
        activeRevision = null
        reentryFloor = null
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        activeScope = null
        activeRevision = null
        reentryFloor = null
        highestRevisionByScope.clear()
    }
}
