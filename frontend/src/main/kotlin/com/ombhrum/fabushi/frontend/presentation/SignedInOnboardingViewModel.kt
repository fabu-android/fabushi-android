package com.ombhrum.fabushi

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private data class SignedInOnboardingRefreshSnapshot(
    val accountSlot: String?,
    val accountScope: String?,
    val hasSeenOnboarding: Boolean,
    val agentCount: Int?,
)

internal class SignedInOnboardingViewModel(application: Application) : AndroidViewModel(application) {
    private val coordinator: AndroidCoordinatorPort = CoordinatorClient.presentation()
    private val operationFence = SignedInOnboardingOperationFence()
    private val mutableState = MutableStateFlow(SignedInOnboardingUiState())
    val state: StateFlow<SignedInOnboardingUiState> = mutableState.asStateFlow()
    private var refreshGeneration = 0L
    private var onboardingAccountScope: String? = null

    fun refresh() {
        val generation = ++refreshGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val auth = coordinator.authStatus()
                    if (!auth.optBoolean("loggedIn", false)) {
                        return@runCatching SignedInOnboardingRefreshSnapshot(
                            accountSlot = null,
                            accountScope = null,
                            hasSeenOnboarding = false,
                            agentCount = 0,
                        )
                    }
                    val user = auth.optJSONObject("user")
                    val accountSlot = auth.optString("authId").trim()
                        .ifBlank { user?.optString("id").orEmpty().trim() }
                        .ifBlank { user?.optString("email").orEmpty().trim() }
                        .takeIf(String::isNotEmpty)
                        ?: error("Signed-in onboarding requires an account identity")
                    val onboarding = coordinator.onboardingSeen()
                    val accountScope = onboarding.optString("accountScope").trim()
                        .takeIf(String::isNotEmpty)
                        ?: error("Signed-in onboarding requires a canonical account scope")
                    val count = runCatching { coordinator.agentList().length() }.getOrNull()
                    SignedInOnboardingRefreshSnapshot(
                        accountSlot = accountSlot,
                        accountScope = accountScope,
                        hasSeenOnboarding = onboarding.optBoolean("hasSeenOnboarding", false),
                        agentCount = count,
                    )
                }
            }
            if (generation != refreshGeneration) return@launch
            result.onSuccess { refresh ->
                val accountSlot = refresh.accountSlot
                if (accountSlot == null) {
                    onboardingAccountScope = null
                    operationFence.setAccount(null)
                    mutableState.value = SignedInOnboardingUiState()
                    return@onSuccess
                }
                onboardingAccountScope = refresh.accountScope
                operationFence.setAccount(accountSlot)
                val shouldOpen = signedInOnboardingShouldOpen(
                    signedIn = true,
                    hasSeenOnboarding = refresh.hasSeenOnboarding,
                    agentCount = refresh.agentCount,
                )
                val current = mutableState.value
                mutableState.value = if (current.accountSlot == accountSlot) {
                    current.copy(active = shouldOpen)
                } else {
                    SignedInOnboardingUiState(active = shouldOpen, accountSlot = accountSlot)
                }
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    active = false,
                    error = error.message ?: "Unable to resolve onboarding account",
                )
            }
        }
    }

    fun resetAccountScope() {
        refreshGeneration += 1
        onboardingAccountScope = null
        operationFence.setAccount(null)
        mutableState.value = SignedInOnboardingUiState()
    }

    fun next() {
        val current = mutableState.value
        if (!current.active || current.creating || current.step == SignedInOnboardingStep.HAND_OFF) return
        mutableState.value = current.copy(step = nextSignedInOnboardingStep(current.step), error = null)
    }

    fun back() {
        val current = mutableState.value
        if (!current.active || current.creating) return
        val previous = previousSignedInOnboardingStep(current.step) ?: return
        mutableState.value = current.copy(step = previous, error = null)
    }

    fun updateDraft(name: String? = null, description: String? = null) {
        val current = mutableState.value
        if (!current.active || current.creating) return
        mutableState.value = current.copy(
            draft = current.draft.copy(
                name = name ?: current.draft.name,
                description = description ?: current.draft.description,
                pickedTemplateId = if (name != null || description != null) null else current.draft.pickedTemplateId,
            ),
            error = null,
        )
    }

    fun choosePersona(shape: String? = null, color: String? = null) {
        val current = mutableState.value
        if (!current.active || current.creating) return
        mutableState.value = current.copy(
            draft = normalizeSignedInOnboardingDraft(
                current.draft.copy(
                    shape = shape ?: current.draft.shape,
                    color = color ?: current.draft.color,
                ),
            ),
            error = null,
        )
    }

    fun toggleDailyTool(tool: String) {
        val current = mutableState.value
        if (!current.active || current.creating || tool !in SIGNED_IN_ONBOARDING_DAILY_TOOLS) return
        val selected = current.selectedDailyTools.toMutableSet()
        if (!selected.add(tool)) selected.remove(tool)
        mutableState.value = current.copy(selectedDailyTools = selected, error = null)
    }

    fun chooseSuggestion(suggestion: SignedInOnboardingSuggestion) {
        val current = mutableState.value
        if (!current.active || current.creating) return
        mutableState.value = current.copy(
            draft = current.draft.copy(
                name = suggestion.name,
                description = suggestion.description,
                pickedTemplateId = suggestion.templateId,
            ),
            step = SignedInOnboardingStep.CREATE,
            error = null,
        )
    }

    fun createFirstAgent() {
        val current = mutableState.value
        val accountSlot = current.accountSlot ?: return
        val expectedAccountScope = onboardingAccountScope ?: return
        val draft = normalizeSignedInOnboardingDraft(current.draft)
        if (!current.active || current.creating || !canSubmitSignedInOnboardingDraft(draft)) return
        val token = operationFence.begin() ?: return
        mutableState.value = current.copy(creating = true, draft = draft, error = null)
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val created = coordinator.agentCreateWithPersona(
                        name = draft.name.replace(Regex("\\s+"), " ").trim().take(72),
                        description = signedInOnboardingDescription(
                            draft.description.trim(),
                            current.selectedDailyTools,
                        ),
                        avatarShape = draft.shape,
                        avatarColor = draft.color,
                    )
                    val markerError = runCatching {
                        val saved = coordinator.setOnboardingSeen(
                            expectedAccountScope = expectedAccountScope,
                            hasSeenOnboarding = true,
                        )
                        check(saved.optBoolean("hasSeenOnboarding", false)) {
                            "Host did not persist onboarding completion"
                        }
                    }.exceptionOrNull()
                    created to markerError
                }
            }
            if (!operationFence.accepts(token, mutableState.value.accountSlot)) return@launch
            result.onSuccess { (created, markerError) ->
                val id = created.optJSONObject("agent")?.optString("id").orEmpty().trim()
                if (id.isEmpty()) {
                    mutableState.value = mutableState.value.copy(
                        creating = false,
                        error = "The created Agent did not return an identity.",
                    )
                    operationFence.finish(token)
                    return@onSuccess
                }
                mutableState.value = mutableState.value.copy(
                    creating = false,
                    step = SignedInOnboardingStep.HAND_OFF,
                    createdAgentId = id,
                    error = if (markerError == null) {
                        null
                    } else {
                        "Your Agent was created, but onboarding completion could not be saved."
                    },
                )
                delay(SIGNED_IN_ONBOARDING_HAND_OFF_DWELL_MS)
                if (operationFence.accepts(token, mutableState.value.accountSlot)) {
                    mutableState.value = mutableState.value.copy(active = false)
                    operationFence.finish(token)
                }
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    creating = false,
                    error = signedInOnboardingCreateError(error),
                )
                operationFence.finish(token)
            }
        }
    }

    override fun onCleared() {
        refreshGeneration += 1
        onboardingAccountScope = null
        operationFence.dispose()
        super.onCleared()
    }
}

internal fun signedInOnboardingCreateError(error: Throwable): String {
    val message = error.message.orEmpty()
    return if (
        message.contains("connect", ignoreCase = true) ||
        message.contains("transport", ignoreCase = true) ||
        message.contains("unavailable", ignoreCase = true)
    ) {
        "Can't reach your computer right now. Check your connection and try again."
    } else {
        message.ifBlank { "Couldn't create your Bot. Try again." }
    }
}
