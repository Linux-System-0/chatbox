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
import androidx.compose.ui.text.style.TextOverflow
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import com.chatbox.android.data.ConversationView
import com.chatbox.android.data.Message
import com.chatbox.android.plugins.ClientPlugins
import java.text.DateFormat
import java.util.Date

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(state: ChatUiState, viewModel: ChatViewModel, onOpenNetworkSettings: () -> Unit) {
	val tr = { key: String -> ClientPlugins.translate(state.plugins, state.locale, key) }
	var draft by remember { mutableStateOf("") }
	var newChat by remember { mutableStateOf("") }
	var languageMenu by remember { mutableStateOf(false) }
	var themeMenu by remember { mutableStateOf(false) }
	var pluginDialog by remember { mutableStateOf(false) }
	var timeSettingsDialog by remember { mutableStateOf(false) }
	val installLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
		uri?.let(viewModel::installPlugin)
	}

	Scaffold(
		topBar = {
			TopAppBar(
				title = {
					Column {
						Text(tr("app_name"))
						state.server?.let { server ->
							Text(
								server.origin,
								style = MaterialTheme.typography.labelSmall,
							)
						}
					}
				},
				actions = {
					TextButton(
						onClick = { timeSettingsDialog = true },
						modifier = Modifier.semantics { contentDescription = tr("time_settings") },
					) { Text("🕒") }
					TextButton(onClick = onOpenNetworkSettings) { Text("⚙") }
					Box {
						TextButton(onClick = { languageMenu = true }) { Text("🌐 ${state.locale}") }
						DropdownMenu(expanded = languageMenu, onDismissRequest = { languageMenu = false }) {
							ClientPlugins.languageOptions(state.plugins).forEach { language ->
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
							ClientPlugins.themeOptions(state.plugins).forEach { theme ->
								DropdownMenuItem(
									text = { Text(if (theme.builtin) tr(theme.id) else theme.label) },
									onClick = {
										viewModel.setTheme(theme.id)
										themeMenu = false
									},
								)
							}
						}
					}
					Box {
						TextButton(onClick = { pluginDialog = true }) { Text("🧩") }
					}
					TextButton(onClick = { viewModel.logout() }) { Text(tr("logout")) }
				},
			)
		},
	) { padding ->
		Column(modifier = Modifier.padding(padding).fillMaxSize()) {
			state.verificationDisabledAt?.let { since ->
				Row(
					modifier = Modifier
						.fillMaxWidth()
						.background(Color(0xFFB00020))
						.padding(12.dp),
					verticalAlignment = Alignment.CenterVertically,
				) {
					Text(
						tr("verification_disabled_banner").replace(
							"{time}",
							DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT)
								.format(Date(since)),
						),
						color = Color.White,
					)
				}
			}
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
				tr(state.status),
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

	if (pluginDialog) {
		PluginManagerDialog(
			plugins = state.plugins,
			locale = state.locale,
			theme = state.theme,
			onInstall = {
				installLauncher.launch(
					arrayOf("application/json", "text/json", "text/plain", "application/octet-stream"),
				)
			},
			onRemove = viewModel::removePlugin,
			onClose = { pluginDialog = false },
			tr = tr,
		)
	}

	if (timeSettingsDialog) {
		TimeSettingsDialog(
			settings = state.timeSettings,
			onSave = {
				viewModel.setTimeSettings(it)
				timeSettingsDialog = false
			},
			onReset = {
				viewModel.resetTimeSettings()
				timeSettingsDialog = false
			},
			onClose = { timeSettingsDialog = false },
			tr = tr,
		)
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
	val conversation = state.conversations.firstOrNull { it.id == state.selectedId }
	val myId = state.session?.user?.id
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
						Row(verticalAlignment = Alignment.CenterVertically) {
							Text(
								conversation.title,
								style = MaterialTheme.typography.titleMedium,
								maxLines = 1,
								overflow = TextOverflow.Ellipsis,
								modifier = Modifier.weight(1f),
							)
							conversation.lastMessage?.let { message ->
								Text(
									TimeFormat.format(message.createdAt, state.timeSettings),
									style = MaterialTheme.typography.labelSmall,
								)
							}
						}
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
	val conversation = state.conversations.firstOrNull { it.id == state.selectedId }
	val myId = state.session?.user?.id
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
				val mine = message.senderId == myId
				val receipt = if (mine) readReceipt(conversation, message, myId, tr) else null
				Row(
					modifier = Modifier.fillMaxWidth(),
					horizontalArrangement = if (mine) Arrangement.End else Arrangement.Start,
				) {
					Column(
						modifier = Modifier
							.width(280.dp)
							.clip(RoundedCornerShape(10.dp))
							.background(
								if (mine) MaterialTheme.colorScheme.primaryContainer
								else MaterialTheme.colorScheme.surfaceVariant,
							)
							.padding(8.dp),
					) {
						Text(message.content ?: "[${message.kind}]")
						Row(
							modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
							horizontalArrangement = Arrangement.End,
							verticalAlignment = Alignment.CenterVertically,
						) {
							Text(
								TimeFormat.format(message.createdAt, state.timeSettings),
								style = MaterialTheme.typography.labelSmall,
								color = if (mine) MaterialTheme.colorScheme.onPrimaryContainer
								else MaterialTheme.colorScheme.onSurfaceVariant,
							)
							receipt?.let {
								Text(
									if (it.read) "✓✓" else "✓",
									style = MaterialTheme.typography.labelSmall,
									color = if (it.read) Color(0xFF12B76A)
									else if (mine) MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.65f)
									else MaterialTheme.colorScheme.onSurfaceVariant,
									modifier = Modifier
										.padding(start = 4.dp)
										.semantics { contentDescription = it.label },
								)
							}
						}
					}
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


private data class ReadReceipt(val read: Boolean, val label: String)

private fun readReceipt(
	conversation: ConversationView?,
	message: Message,
	myId: Long?,
	tr: (String) -> String,
): ReadReceipt? {
	if (conversation == null) return null
	val others = conversation.readCursors.filter { it.userId != myId }
	if (conversation.kind == "direct") {
		val peer = others.firstOrNull() ?: return ReadReceipt(false, tr("unread"))
		val read = peer.lastReadSeq >= message.seq
		return ReadReceipt(read, if (read) tr("read") else tr("unread"))
	}
	if (others.isEmpty()) return ReadReceipt(true, tr("read"))
	val read = others.count { it.lastReadSeq >= message.seq }
	return ReadReceipt(
		read == others.size,
		tr("read_by_count")
			.replace("{read}", read.toString())
			.replace("{total}", others.size.toString()),
	)
}
