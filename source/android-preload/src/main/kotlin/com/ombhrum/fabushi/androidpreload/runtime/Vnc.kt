package com.ombhrum.fabushi.androidpreload.runtime
data class VncRuntime(val sessionId: String, val visible: Boolean) { init { require(sessionId.isNotBlank()) } }
