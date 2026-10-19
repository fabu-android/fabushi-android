package com.ombhrum.fabushi.androidmain.auth

/**
 * Android adaptation of desktop protocol registration.
 *
 * Android protocol ownership is declared by the packaged manifest, not registered dynamically.
 * This contract is the single runtime source for the scheme accepted by the Intent router.
 */
internal object AndroidAuthCallbackRegistration {
    const val REDIRECT_TARGET = "fabushi"
    const val PROTOCOL_SCHEME = "fabushi"

    private val protocolToken = Regex("^[a-z][a-z0-9+.-]{1,31}$")

    fun validateConfiguredScheme(value: String?): String {
        val normalized = value?.trim()?.lowercase().orEmpty()
        val scheme = normalized.ifBlank { PROTOCOL_SCHEME }
        require(protocolToken.matches(scheme)) { "Invalid Android auth callback scheme" }
        return scheme
    }

    fun registration(): AndroidAuthCallbackRegistrationState =
        AndroidAuthCallbackRegistrationState(
            redirectTarget = REDIRECT_TARGET,
            protocolScheme = PROTOCOL_SCHEME,
            registered = true,
            registrationSource = "android-manifest",
        )
}

internal data class AndroidAuthCallbackRegistrationState(
    val redirectTarget: String,
    val protocolScheme: String,
    val registered: Boolean,
    val registrationSource: String,
)
