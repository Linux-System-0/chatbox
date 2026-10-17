package com.chatbox.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.chatbox.android.network.AllowlistEntry
import com.chatbox.android.network.AllowlistKind
import com.chatbox.android.network.Certificates
import java.text.DateFormat
import java.util.Date

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NetworkSettingsScreen(
	state: ChatUiState,
	onTrustSystemCAs: (Boolean) -> Unit,
	onSetVerificationDisabled: (Boolean) -> Unit,
	onAddAllowlistEntry: (AllowlistKind, String) -> Unit,
	onRemoveAllowlistEntry: (AllowlistEntry) -> Unit,
	onImportCa: () -> Unit,
	onRemoveCa: (com.chatbox.android.network.CertificateInfo) -> Unit,
	onClose: () -> Unit,
	tr: (String) -> String,
) {
	var entryKind by remember { mutableStateOf(AllowlistKind.DNS) }
	var entryText by remember { mutableStateOf("") }
	var confirmDisable by remember { mutableStateOf(false) }

	Scaffold(
		topBar = {
			TopAppBar(
				title = { Text(tr("network_settings")) },
				navigationIcon = { TextButton(onClick = onClose) { Text("←") } },
			)
		},
	) { padding ->
		LazyColumn(
			modifier = Modifier.padding(padding).fillMaxSize(),
			verticalArrangement = Arrangement.spacedBy(8.dp),
		) {
			item {
				SettingsHeader(tr("trust_system_cas"), tr("trust_system_cas_hint"))
				Row(
					modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
					verticalAlignment = Alignment.CenterVertically,
				) {
					Text(tr("trust_system_cas"), modifier = Modifier.weight(1f))
					Switch(
						checked = state.policy.trustSystemCAs,
						onCheckedChange = onTrustSystemCAs,
					)
				}
			}

			item {
				SettingsHeader(tr("disable_verification"), tr("disable_verification_hint"))
				Row(
					modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
					verticalAlignment = Alignment.CenterVertically,
				) {
					Text(tr("disable_verification"), modifier = Modifier.weight(1f))
					Switch(
						checked = state.policy.verificationDisabled,
						onCheckedChange = { enabled ->
							if (enabled) confirmDisable = true else onSetVerificationDisabled(false)
						},
					)
				}
				state.verificationDisabledAt?.let { since ->
					Text(
						tr("verification_disabled_banner").replace(
							"{time}",
							DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT)
								.format(Date(since)),
						),
						color = Color.White,
						modifier = Modifier
							.fillMaxWidth()
							.background(Color(0xFFB00020))
							.padding(12.dp),
					)
				}
			}

			item {
				SettingsHeader(tr("allow_cleartext"), tr("allow_cleartext_hint"))
				Row(
					modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
					horizontalArrangement = Arrangement.spacedBy(8.dp),
				) {
					FilterChip(
						selected = entryKind == AllowlistKind.DNS,
						onClick = { entryKind = AllowlistKind.DNS },
						label = { Text(tr("allowlist_kind_dns")) },
					)
					FilterChip(
						selected = entryKind == AllowlistKind.IP,
						onClick = { entryKind = AllowlistKind.IP },
						label = { Text(tr("allowlist_kind_ip")) },
					)
					FilterChip(
						selected = entryKind == AllowlistKind.CIDR,
						onClick = { entryKind = AllowlistKind.CIDR },
						label = { Text(tr("allowlist_kind_cidr")) },
					)
				}
				Row(
					modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
					horizontalArrangement = Arrangement.spacedBy(8.dp),
					verticalAlignment = Alignment.CenterVertically,
				) {
					OutlinedTextField(
						value = entryText,
						onValueChange = { entryText = it },
						label = { Text(tr("allow_cleartext")) },
						singleLine = true,
						modifier = Modifier.weight(1f),
					)
					Button(
						onClick = {
							onAddAllowlistEntry(entryKind, entryText)
							entryText = ""
						},
						enabled = entryText.isNotBlank(),
					) {
						Text(tr("allowlist_add"))
					}
				}
			}

			if (state.policy.allowlist.isEmpty()) {
				item {
					Text(
						tr("allowlist_empty"),
						modifier = Modifier.padding(horizontal = 16.dp),
						style = MaterialTheme.typography.bodySmall,
					)
				}
			} else {
				items(state.policy.allowlist, key = { it.serialized }) { entry ->
					Row(
						modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
						verticalAlignment = Alignment.CenterVertically,
					) {
						Text(entry.value, modifier = Modifier.weight(1f))
						TextButton(onClick = { onRemoveAllowlistEntry(entry) }) { Text(tr("remove")) }
					}
					HorizontalDivider()
				}
			}

			item {
				SettingsHeader(tr("custom_cas"), tr("custom_cas_hint"))
				TextButton(
					onClick = onImportCa,
					modifier = Modifier.padding(horizontal = 8.dp),
				) {
					Text(tr("import_ca"))
				}
			}

			if (state.policy.customCertificates.isEmpty()) {
				item {
					Text(
						tr("no_custom_cas"),
						modifier = Modifier.padding(horizontal = 16.dp),
						style = MaterialTheme.typography.bodySmall,
					)
				}
			} else {
				items(state.policy.customCertificates, key = { Certificates.sha256(it) }) { certificate ->
					val info = remember(certificate) { Certificates.info(certificate) }
					Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
						CertificateDetails(info, tr)
						TextButton(onClick = { onRemoveCa(info) }) { Text(tr("remove")) }
					}
					HorizontalDivider()
				}
			}

			item {
				state.error?.let { message ->
					Text(
						message,
						color = MaterialTheme.colorScheme.error,
						modifier = Modifier.padding(16.dp),
					)
				}
			}
		}
	}

	if (confirmDisable) {
		AlertDialog(
			onDismissRequest = { confirmDisable = false },
			title = { Text(tr("verification_disabled_title")) },
			text = { Text(tr("verification_disabled_body")) },
			confirmButton = {
				TextButton(
					onClick = {
						confirmDisable = false
						onSetVerificationDisabled(true)
					},
				) {
					Text(tr("verification_disabled_confirm"), color = MaterialTheme.colorScheme.error)
				}
			},
			dismissButton = {
				TextButton(onClick = { confirmDisable = false }) { Text(tr("cancel")) }
			},
		)
	}
}

@Composable
private fun SettingsHeader(title: String, hint: String) {
	Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
		Text(title, style = MaterialTheme.typography.titleMedium)
		Text(hint, style = MaterialTheme.typography.bodySmall)
	}
}
