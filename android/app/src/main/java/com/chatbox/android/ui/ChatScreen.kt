package com.chatbox.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.chatbox.android.plugins.ClientPlugins

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(state: ChatUiState, viewModel: ChatViewModel) {
	val tr = { key: String -> ClientPlugins.translate(state.locale, key) }
	var draft by remember { mutableStateOf("") }
	var newChat by remember { mutableStateOf("") }
	var languageMenu by remember { mutableStateOf(false) }
	var themeMenu by remember { mutableStateOf(false) }

	Scaffold(
		topBar = {
			TopAppBar(
				title = { Text(tr("app_name")) },
				actions = {
					Box {
						TextButton(onClick = { languageMenu = true }) { Text("🌐 ${state.locale}") }
						DropdownMenu(expanded = languageMenu, onDismissRequest = { languageMenu = false }) {
							ClientPlugins.languages.forEach { language ->
								DropdownMenuItem(
									text = { Text(language.label) },
									onClick = {
										viewModel.setLocale(language.code)
										languageMenu = false
									},
								)
							}
						}
					}
					Box {
						TextButton(onClick = { themeMenu = true }) { Text("🎨") }
						DropdownMenu(expanded = themeMenu, onDismissRequest = { themeMenu = false }) {
							ClientPlugins.themes.forEach { theme ->
								DropdownMenuItem(
									text = { Text(theme) },
									onClick = {
										viewModel.setTheme(theme)
										themeMenu = false
									},
								)
							}
						}
					}
					TextButton(onClick = { viewModel.logout() }) { Text(tr("logout")) }
				},
			)
		},
	) { padding ->
		Column(modifier = Modifier.padding(padding).fillMaxSize()) {
			state.error?.let { message ->
				Row(
					modifier = Modifier
						.fillMaxWidth()
						.background(MaterialTheme.colorScheme.errorContainer)
						.padding(8.dp),
					verticalAlignment = Alignment.CenterVertically,
				) {
					Text(message, modifier = Modifier.weight(1f))
					TextButton(onClick = { viewModel.dismissError() }) { Text("×") }
				}
			}
			Text(
				"status: ${state.status}",
				modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
				style = MaterialTheme.typography.labelSmall,
			)

			if (state.selectedId == null) {
				ConversationList(
					state = state,
					newChat = newChat,
					onNewChatChange = { newChat = it },
					onStartChat = {
						if (newChat.isNotBlank()) {
							viewModel.startDirect(newChat.trim())
							newChat = ""
						}
					},
					onSelect = { viewModel.select(it) },
					tr = tr,
				)
			} else {
				MessagePane(
					state = state,
					draft = draft,
					onDraftChange = { draft = it },
					onBack = { viewModel.closeConversation() },
					onSend = {
						viewModel.send(draft)
						draft = ""
					},
					tr = tr,
				)
			}
		}
	}
}

@Composable
private fun ConversationList(
	state: ChatUiState,
	newChat: String,
	onNewChatChange: (String) -> Unit,
	onStartChat: () -> Unit,
	onSelect: (Long) -> Unit,
	tr: (String) -> String,
) {
	Column(modifier = Modifier.fillMaxSize()) {
		Row(
			modifier = Modifier.fillMaxWidth().padding(8.dp),
			horizontalArrangement = Arrangement.spacedBy(8.dp),
			verticalAlignment = Alignment.CenterVertically,
		) {
			OutlinedTextField(
				value = newChat,
				onValueChange = onNewChatChange,
				label = { Text(tr("new_chat")) },
				singleLine = true,
				modifier = Modifier.weight(1f),
			)
			TextButton(onClick = onStartChat) { Text("+") }
		}
		HorizontalDivider()
		LazyColumn(modifier = Modifier.fillMaxSize()) {
			items(state.conversations, key = { it.id }) { conversation ->
				Row(
					modifier = Modifier
						.fillMaxWidth()
						.clickable { onSelect(conversation.id) }
						.padding(horizontal = 16.dp, vertical = 12.dp),
					verticalAlignment = Alignment.CenterVertically,
				) {
					Column(modifier = Modifier.weight(1f)) {
						Text(conversation.title, style = MaterialTheme.typography.titleMedium)
						Text(
							conversation.lastMessage?.content ?: "",
							style = MaterialTheme.typography.bodySmall,
							maxLines = 1,
						)
					}
					if (conversation.unread > 0) {
						Text(
							conversation.unread.toString(),
							color = MaterialTheme.colorScheme.primary,
							style = MaterialTheme.typography.labelLarge,
						)
					}
				}
				HorizontalDivider()
			}
		}
	}
}

@Composable
private fun MessagePane(
	state: ChatUiState,
	draft: String,
	onDraftChange: (String) -> Unit,
	onBack: () -> Unit,
	onSend: () -> Unit,
	tr: (String) -> String,
) {
	Column(modifier = Modifier.fillMaxSize()) {
		Row(
			modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp),
			verticalAlignment = Alignment.CenterVertically,
		) {
			TextButton(onClick = onBack) { Text("←") }
			Text(
				state.conversations.firstOrNull { it.id == state.selectedId }?.title ?: "",
				style = MaterialTheme.typography.titleMedium,
			)
		}
		HorizontalDivider()
		LazyColumn(
			modifier = Modifier.weight(1f).fillMaxWidth().padding(8.dp),
			verticalArrangement = Arrangement.spacedBy(6.dp),
		) {
			items(state.messages, key = { it.id }) { message ->
				val mine = message.senderId == state.session?.user?.id
				Row(
					modifier = Modifier.fillMaxWidth(),
					horizontalArrangement = if (mine) Arrangement.End else Arrangement.Start,
				) {
					Text(
						text = message.content ?: "[${message.kind}]",
						modifier = Modifier
							.width(280.dp)
							.clip(RoundedCornerShape(10.dp))
							.background(
								if (mine) MaterialTheme.colorScheme.primaryContainer
								else MaterialTheme.colorScheme.surfaceVariant,
							)
							.padding(8.dp),
					)
				}
			}
		}
		HorizontalDivider()
		Row(
			modifier = Modifier.fillMaxWidth().padding(8.dp),
			horizontalArrangement = Arrangement.spacedBy(8.dp),
			verticalAlignment = Alignment.CenterVertically,
		) {
			OutlinedTextField(
				value = draft,
				onValueChange = onDraftChange,
				placeholder = { Text(tr("message_placeholder")) },
				maxLines = 3,
				modifier = Modifier.weight(1f),
			)
			TextButton(onClick = onSend) { Text(tr("send")) }
		}
	}
}
