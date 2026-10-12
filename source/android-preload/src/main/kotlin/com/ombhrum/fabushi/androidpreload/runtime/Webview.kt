package com.ombhrum.fabushi.androidpreload.runtime
data class WebviewRuntime(val allowedOrigins: Set<String>) { init { require(allowedOrigins.none { it.isBlank() }) } }
