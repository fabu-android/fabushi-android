package com.ombhrum.fabushi.androidmain.webauthn

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.credentials.CreatePublicKeyCredentialRequest
import androidx.credentials.CreatePublicKeyCredentialResponse
import androidx.credentials.CredentialManager
import androidx.credentials.GetCredentialRequest
import androidx.credentials.GetPublicKeyCredentialOption
import androidx.credentials.PublicKeyCredential
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject

internal data class AndroidWebAuthnCeremony(
    val requestId: String,
    val kind: String,
    val origin: String,
    val payloadJson: String,
) {
    init {
        require(requestId.isNotBlank()) { "requestId is required" }
        require(kind == "create" || kind == "get") { "kind must be create or get" }
        require(origin.isNotBlank()) { "origin is required" }
        JSONObject(payloadJson)
    }

    companion object {
        fun fromFrame(frame: JSONObject): AndroidWebAuthnCeremony? {
            if (frame.optString("kind") != "ceremony") return null
            val requestId = frame.optString("requestId").trim()
            val ceremony = frame.optJSONObject("ceremony") ?: return null
            val kind = ceremony.optString("kind").trim().lowercase()
            val origin = ceremony.optString("origin").trim()
            val payloadJson = ceremony.optString("payloadJson").trim()
            if (requestId.isBlank() || origin.isBlank() || payloadJson.isBlank()) return null
            if (kind != "create" && kind != "get") return null
            return runCatching {
                AndroidWebAuthnCeremony(requestId, kind, origin, payloadJson)
            }.getOrNull()
        }
    }
}

/**
 * Android-native WebAuthn provider driven by the Host/Coordinator WebAuthn bridge.
 *
 * It owns no Host state. Registration, liveness, request delivery and settlement live behind
 * [AndroidCoordinatorPort]. This adapter only turns a typed ceremony into Credential Manager UI.
 */
internal class AndroidCredentialManagerWebAuthn(
    application: Application,
    private val coordinator: AndroidCoordinatorPort,
) : AutoCloseable {
    private val credentialManager = CredentialManager.create(application)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    @Volatile
    private var attachedActivity: ComponentActivity? = null

    @Volatile
    private var providerId: String? = null

    private var providerLoop: Job? = null
    private var activeCeremony: Job? = null

    fun attach(activity: ComponentActivity) {
        if (attachedActivity === activity && providerLoop?.isActive == true) return
        detach(attachedActivity)
        attachedActivity = activity
        providerLoop = scope.launch {
            runProviderLoop(activity)
        }
    }

    fun detach(activity: ComponentActivity?) {
        if (activity != null && attachedActivity !== activity) return
        attachedActivity = null
        activeCeremony?.cancel()
        activeCeremony = null
        providerLoop?.cancel()
        providerLoop = null
        val id = providerId
        providerId = null
        if (!id.isNullOrBlank()) {
            scope.launch(Dispatchers.IO) {
                runCatching {
                    coordinator.webAuthnUnregisterProvider(
                        JSONObject().put("providerId", id),
                    )
                }
            }
        }
    }

    private suspend fun runProviderLoop(activity: ComponentActivity) {
        val registered = runCatching {
            coordinator.webAuthnRegisterProvider()
        }.getOrElse {
            return
        }
        val id = registered.optString("providerId").trim()
        if (id.isBlank()) return
        providerId = id

        submitFrames(
            id,
            listOf(
                JSONObject()
                    .put("kind", "hello")
                    .put("label", "Fabushi Android"),
                JSONObject().put("kind", "ping"),
            ),
        )

        var lastHeartbeatAt = System.currentTimeMillis()
        while (scope.isActive && attachedActivity === activity && providerId == id) {
            val polled = runCatching {
                coordinator.webAuthnPollRequest(
                    JSONObject().put("providerId", id),
                )
            }.getOrNull()
            val frame = polled?.optJSONObject("frame")
            if (frame != null) {
                when (frame.optString("kind")) {
                    "welcome" -> {
                        submitFrames(
                            id,
                            listOf(
                                JSONObject()
                                    .put("kind", "hello")
                                    .put("label", "Fabushi Android"),
                                JSONObject().put("kind", "ping"),
                            ),
                        )
                        lastHeartbeatAt = System.currentTimeMillis()
                    }
                    "ceremony" -> AndroidWebAuthnCeremony.fromFrame(frame)?.let { ceremony ->
                        activeCeremony?.cancel()
                        activeCeremony = scope.launch {
                            performCeremony(activity, id, ceremony)
                        }
                    }
                    "cancel" -> {
                        val requestId = frame.optString("requestId")
                        activeCeremony?.cancel(CancellationException("Host cancelled $requestId"))
                        activeCeremony = null
                    }
                }
            }

            val now = System.currentTimeMillis()
            if (now - lastHeartbeatAt >= HEARTBEAT_INTERVAL_MS) {
                submitFrames(id, listOf(JSONObject().put("kind", "ping")))
                lastHeartbeatAt = now
            }
            delay(POLL_INTERVAL_MS)
        }
    }

    private suspend fun performCeremony(
        activity: ComponentActivity,
        providerId: String,
        ceremony: AndroidWebAuthnCeremony,
    ) {
        try {
            val credentialJson = when (ceremony.kind) {
                "get" -> {
                    val option = GetPublicKeyCredentialOption(
                        requestJson = ceremony.payloadJson,
                    )
                    val response = credentialManager.getCredential(
                        context = activity,
                        request = GetCredentialRequest(listOf(option)),
                    )
                    val publicKey = response.credential as? PublicKeyCredential
                        ?: error("Credential Manager returned a non-public-key credential")
                    publicKey.authenticationResponseJson
                }
                "create" -> {
                    val response = credentialManager.createCredential(
                        context = activity,
                        request = CreatePublicKeyCredentialRequest(
                            requestJson = ceremony.payloadJson,
                        ),
                    )
                    val publicKey = response as? CreatePublicKeyCredentialResponse
                        ?: error("Credential Manager returned a non-public-key creation response")
                    publicKey.registrationResponseJson
                }
                else -> error("Unsupported WebAuthn ceremony kind")
            }

            submitFrames(
                providerId,
                listOf(
                    stageFrame(ceremony.requestId, "grant", "ok"),
                    stageFrame(ceremony.requestId, "sign", "ok"),
                    JSONObject()
                        .put("kind", "result")
                        .put("requestId", ceremony.requestId)
                        .put("credentialJson", credentialJson),
                ),
            )
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            submitFrames(
                providerId,
                listOf(
                    stageFrame(ceremony.requestId, "sign", "failed"),
                    JSONObject()
                        .put("kind", "error")
                        .put("requestId", ceremony.requestId)
                        .put("name", error::class.java.simpleName.ifBlank { "CredentialManagerError" })
                        .put("message", error.message ?: "Credential Manager ceremony failed"),
                ),
            )
        } finally {
            activeCeremony = null
        }
    }

    private fun submitFrames(providerId: String, frames: List<JSONObject>) {
        if (frames.isEmpty()) return
        runCatching {
            coordinator.webAuthnSubmitResponses(
                JSONObject()
                    .put("providerId", providerId)
                    .put("frames", JSONArray().apply { frames.forEach(::put) }),
            )
        }
    }

    override fun close() {
        detach(attachedActivity)
        scope.cancel()
    }

    companion object {
        internal const val HEARTBEAT_INTERVAL_MS = 10_000L
        internal const val POLL_INTERVAL_MS = 250L

        internal fun stageFrame(
            requestId: String,
            stage: String,
            outcome: String,
        ): JSONObject = JSONObject()
            .put("kind", "stage")
            .put("requestId", requestId)
            .put("stage", stage)
            .put("outcome", outcome)
    }
}
