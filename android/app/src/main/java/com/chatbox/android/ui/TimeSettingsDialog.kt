package com.chatbox.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import java.time.Instant

@Composable
fun TimeSettingsDialog(
	settings: TimeDisplaySettings,
	onSave: (TimeDisplaySettings) -> Unit,
	onReset: () -> Unit,
	onClose: () -> Unit,
	tr: (String) -> String,
) {
	val normalized = remember(settings) { normalizeTimeSettings(settings) }
	var zone by remember(normalized) { mutableStateOf(normalized.timeZone) }
	var custom by remember(normalized) { mutableStateOf(normalized.format.isNotBlank()) }
	var format by remember(normalized) { mutableStateOf(normalized.format) }
	var zoneMenu by remember { mutableStateOf(false) }
	val options = remember(tr("time_zone_system")) {
		TimeFormat.timeZoneOptions(tr("time_zone_system"))
	}
	val selectedLabel = options.firstOrNull { it.value == zone }?.label ?: zone
	val draft = TimeDisplaySettings(zone, if (custom) format else "")
	val preview = TimeFormat.format(Instant.now().toString(), draft)

	AlertDialog(
		onDismissRequest = onClose,
		title = { Text(tr("time_settings")) },
		text = {
			Column(
				modifier = Modifier.verticalScroll(rememberScrollState()),
				verticalArrangement = Arrangement.spacedBy(10.dp),
			) {
				Text(tr("time_zone"), style = MaterialTheme.typography.labelLarge)
				Box {
					TextButton(onClick = { zoneMenu = true }, modifier = Modifier.fillMaxWidth()) {
						Text(selectedLabel)
					}
					DropdownMenu(expanded = zoneMenu, onDismissRequest = { zoneMenu = false }) {
						options.forEach { option ->
							DropdownMenuItem(
								text = { Text(option.label) },
								onClick = {
									zone = option.value
									zoneMenu = false
								},
							)
						}
					}
				}
				Text(tr("time_zone_hint"), style = MaterialTheme.typography.bodySmall)

				Text(tr("time_format"), style = MaterialTheme.typography.labelLarge)
				Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
					FilterChip(
						selected = !custom,
						onClick = { custom = false },
						label = { Text(tr("time_format_system")) },
					)
					FilterChip(
						selected = custom,
						onClick = { custom = true },
						label = { Text(tr("time_format_custom")) },
					)
				}
				if (custom) {
					OutlinedTextField(
						value = format,
						onValueChange = { format = it },
						label = { Text(tr("time_pattern")) },
						placeholder = { Text("YYYY-MM-DD HH:mm:ss") },
						singleLine = true,
						modifier = Modifier.fillMaxWidth(),
					)
					Text(tr("time_format_hint"), style = MaterialTheme.typography.bodySmall)
				}

				Column(
					modifier = Modifier
						.fillMaxWidth()
						.padding(top = 4.dp),
					verticalArrangement = Arrangement.spacedBy(2.dp),
				) {
					Text(tr("time_preview"), style = MaterialTheme.typography.labelSmall)
					Text(preview, style = MaterialTheme.typography.bodyMedium)
				}
			}
		},
		confirmButton = {
			TextButton(onClick = { onSave(draft) }) { Text(tr("save")) }
		},
		dismissButton = {
			Row {
				TextButton(onClick = onReset) { Text(tr("reset_defaults")) }
				TextButton(onClick = onClose) { Text(tr("cancel")) }
			}
		},
	)
}
