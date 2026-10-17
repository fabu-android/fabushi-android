package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

@Composable
internal fun SignedInOnboardingSurface(
    state: SignedInOnboardingUiState,
    onNext: () -> Unit,
    onBack: () -> Unit,
    onNameChange: (String) -> Unit,
    onDescriptionChange: (String) -> Unit,
    onToggleTool: (String) -> Unit,
    onChooseSuggestion: (SignedInOnboardingSuggestion) -> Unit,
    onChooseShape: (String) -> Unit,
    onChooseColor: (String) -> Unit,
    onCreate: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(horizontal = 24.dp, vertical = 28.dp)
            .testTag("signed-in-onboarding"),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        when (state.step) {
            SignedInOnboardingStep.MEET -> {
                Text("Meet your new team")
                Text("Create a Bot that can work across your tools and keep going while you are away.")
                Navigation(false, onBack, onNext)
            }
            SignedInOnboardingStep.COMPUTER_DEMO -> {
                Text("Your Bot gets a computer")
                Text("Fabushi gives your Bot an isolated work surface for browser and tool work.")
                Navigation(true, onBack, onNext)
            }
            SignedInOnboardingStep.JOBS -> {
                Text("What should your Bot do?")
                LazyColumn(
                    modifier = Modifier.weight(1f, fill = false),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(selectSignedInOnboardingSuggestions(state.selectedDailyTools, 6), key = { it.templateId }) { suggestion ->
                        TextButton(
                            onClick = { onChooseSuggestion(suggestion) },
                            modifier = Modifier.fillMaxWidth()
                                .testTag("signed-in-suggestion-" + suggestion.templateId),
                        ) {
                            Column(Modifier.fillMaxWidth()) {
                                Text(suggestion.name)
                                Text(suggestion.description)
                            }
                        }
                    }
                }
                Navigation(true, onBack, onNext)
            }
            SignedInOnboardingStep.TOOLS -> {
                Text("What tools do you use every day?")
                var query by remember { mutableStateOf("") }
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    label = { Text("Find a tool") },
                    modifier = Modifier.fillMaxWidth().testTag("signed-in-tools-search"),
                    singleLine = true,
                )
                LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    items(filterSignedInOnboardingTools(query), key = { it }) { tool ->
                        val selected = tool in state.selectedDailyTools
                        TextButton(
                            onClick = { onToggleTool(tool) },
                            modifier = Modifier.fillMaxWidth()
                                .testTag("signed-in-tool-" + tool.replace(" ", "-")),
                        ) { Text(if (selected) "✓ " + tool else tool, modifier = Modifier.fillMaxWidth()) }
                    }
                }
                Navigation(true, onBack, onNext)
            }
            SignedInOnboardingStep.CREATE -> {
                Text("Create your Bot")
                OutlinedTextField(
                    value = state.draft.name,
                    onValueChange = onNameChange,
                    label = { Text("Bot name") },
                    modifier = Modifier.fillMaxWidth().testTag("signed-in-bot-name"),
                    singleLine = true,
                )
                OutlinedTextField(
                    value = state.draft.description,
                    onValueChange = onDescriptionChange,
                    label = { Text("What should this Bot do?") },
                    modifier = Modifier.fillMaxWidth().testTag("signed-in-bot-description"),
                    minLines = 3,
                    maxLines = 5,
                )
                Text("Shape")
                LazyColumn(Modifier.height(90.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    items(SIGNED_IN_ONBOARDING_SHAPES, key = { it }) { shape ->
                        TextButton(
                            onClick = { onChooseShape(shape) },
                            modifier = Modifier.testTag("signed-in-shape-" + shape),
                        ) { Text(if (state.draft.shape == shape) "✓ " + shape else shape) }
                    }
                }
                Text("Color")
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    SIGNED_IN_ONBOARDING_COLORS.keys.take(5).forEach { color ->
                        TextButton(
                            onClick = { onChooseColor(color) },
                            modifier = Modifier.testTag("signed-in-color-" + color),
                        ) { Text(if (state.draft.color == color) "✓ " + color else color) }
                    }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    SIGNED_IN_ONBOARDING_COLORS.keys.drop(5).forEach { color ->
                        TextButton(
                            onClick = { onChooseColor(color) },
                            modifier = Modifier.testTag("signed-in-color-" + color),
                        ) { Text(if (state.draft.color == color) "✓ " + color else color) }
                    }
                }
                state.error?.let { Text(it, modifier = Modifier.testTag("signed-in-onboarding-error")) }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    TextButton(onClick = onBack, enabled = !state.creating) { Text("Back") }
                    Button(
                        onClick = onCreate,
                        enabled = canSubmitSignedInOnboardingDraft(state.draft) && !state.creating,
                        modifier = Modifier.testTag("signed-in-create"),
                    ) {
                        if (state.creating) CircularProgressIndicator() else Text("Create Bot")
                    }
                }
            }
            SignedInOnboardingStep.HAND_OFF -> {
                Spacer(Modifier.weight(1f))
                CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally))
                Text(
                    signedInOnboardingHandoffStatus(false, null, null),
                    modifier = Modifier.align(Alignment.CenterHorizontally).testTag("signed-in-handoff"),
                )
                Spacer(Modifier.weight(1f))
            }
        }
    }
}

@Composable
private fun Navigation(canBack: Boolean, onBack: () -> Unit, onNext: () -> Unit) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        if (canBack) TextButton(onClick = onBack) { Text("Back") } else Spacer(Modifier)
        Button(onClick = onNext, modifier = Modifier.testTag("signed-in-next")) { Text("Continue") }
    }
}
