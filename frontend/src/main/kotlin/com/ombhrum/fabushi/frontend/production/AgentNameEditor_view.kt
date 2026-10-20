package com.ombhrum.fabushi

import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction

@Composable
internal fun AgentNameEditor(
    initialValue: String,
    onCommit: (String) -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var draft by remember(initialValue) { mutableStateOf(initialValue) }
    var cancelled by remember(initialValue) { mutableStateOf(false) }
    var finished by remember(initialValue) { mutableStateOf(false) }
    var hadFocus by remember(initialValue) { mutableStateOf(false) }
    val requester = remember { FocusRequester() }

    fun finish(commit: Boolean) {
        if (finished) return
        finished = true
        if (commit) {
            committedAgentName(initialValue, draft)?.let(onCommit)
        }
        onExit()
    }

    LaunchedEffect(initialValue) {
        requester.requestFocus()
    }

    OutlinedTextField(
        value = draft,
        onValueChange = { draft = it },
        singleLine = true,
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { finish(!cancelled) }),
        modifier = modifier
            .focusRequester(requester)
            .onFocusChanged { state ->
                if (state.isFocused) {
                    hadFocus = true
                } else if (hadFocus) {
                    finish(!cancelled)
                }
            }
            .onPreviewKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                when (event.key) {
                    Key.Enter -> {
                        finish(!cancelled)
                        true
                    }
                    Key.Escape -> {
                        cancelled = true
                        finish(false)
                        true
                    }
                    else -> false
                }
            }
            .testTag("agent-name-editor"),
    )
}
