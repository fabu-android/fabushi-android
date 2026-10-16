package com.ombhrum.fabushi.androidpreload
interface DevControlsTransport { fun invoke(command: String, payload: String): String }
class PreloadDevControls(private val transport: DevControlsTransport) {
    fun invoke(command: String, payload: String = "{}"): String {
        require(command.isNotBlank()) { "dev command must not be blank" }
        return transport.invoke(command, payload)
    }
}
