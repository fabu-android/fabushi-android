package com.ombhrum.fabushi.androidpreload.runtime
data class DevControlsRuntime(val enabled: Boolean) {
    fun requireEnabled() { check(enabled) { "developer controls are disabled" } }
}
