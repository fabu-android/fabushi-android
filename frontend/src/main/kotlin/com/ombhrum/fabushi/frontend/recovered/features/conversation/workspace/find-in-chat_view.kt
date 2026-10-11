package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

@Composable
internal fun FindInChatBar(
    query: String,
    matchCount: Int,
    currentIndex: Int,
    onQueryChange: (String) -> Unit,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    onClose: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OutlinedTextField(
            value = query,
            onValueChange = onQueryChange,
            modifier = Modifier
                .weight(1f)
                .testTag("find-in-chat-input"),
            singleLine = true,
            placeholder = { Text("搜索此聊天", color = homeSecondaryText) },
            colors = OutlinedTextFieldDefaults.colors(
                focusedTextColor = homePrimaryText,
                unfocusedTextColor = homePrimaryText,
                focusedContainerColor = homeSurface,
                unfocusedContainerColor = homeSurface,
            ),
            shape = androidx.compose.foundation.shape.RoundedCornerShape(14.dp),
        )
        if (query.isNotBlank()) {
            Text(
                if (matchCount == 0 || currentIndex < 0) "0/0" else "${currentIndex + 1}/$matchCount",
                color = homeSecondaryText,
                modifier = Modifier.padding(horizontal = 6.dp),
            )
        }
        TextButton(
            enabled = matchCount > 0,
            onClick = onPrevious,
            modifier = Modifier.testTag("find-in-chat-previous"),
        ) { Text("↑") }
        TextButton(
            enabled = matchCount > 0,
            onClick = onNext,
            modifier = Modifier.testTag("find-in-chat-next"),
        ) { Text("↓") }
        TextButton(
            onClick = onClose,
            modifier = Modifier.testTag("find-in-chat-close"),
        ) { Text("×") }
    }
}
