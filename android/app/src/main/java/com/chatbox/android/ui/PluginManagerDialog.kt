package com.chatbox.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.chatbox.android.plugins.ClientPlugin

/** Lists the current account's client plugins and installs/removes them. */
@Composable
fun PluginManagerDialog(
	plugins: List<ClientPlugin>,
	locale: String,
	theme: String,
	onInstall: () -> Unit,
	onRemove: (String) -> Unit,
	onClose: () -> Unit,
	tr: (String) -> String,
) {
	AlertDialog(
		onDismissRequest = onClose,
		title = { Text(tr("plugins")) },
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
				Text(tr("plugin_install_hint"), style = MaterialTheme.typography.bodySmall)
				if (plugins.isEmpty()) {
					Text(tr("no_plugins"), style = MaterialTheme.typography.bodyMedium)
				} else {
					LazyColumn(
						modifier = Modifier.heightIn(max = 320.dp),
						verticalArrangement = Arrangement.spacedBy(8.dp),
					) {
						items(plugins, key = { it.name }) { plugin ->
							Row(verticalAlignment = Alignment.CenterVertically) {
								Column(modifier = Modifier.weight(1f)) {
									Text(pluginLabel(plugin), style = MaterialTheme.typography.bodyMedium)
									val meta = buildString {
										append(kindLabel(plugin, tr))
										append(" · v")
										append(plugin.version)
										if (isActive(plugin, locale, theme)) {
											append(" · ")
											append(tr("active"))
										}
									}
									Text(meta, style = MaterialTheme.typography.labelSmall)
									if (plugin.description.isNotBlank()) {
										Text(
											plugin.description,
											modifier = Modifier.padding(top = 2.dp),
											style = MaterialTheme.typography.bodySmall,
										)
									}
								}
								TextButton(onClick = { onRemove(plugin.name) }) { Text(tr("remove")) }
							}
						}
					}
				}
			}
		},
		confirmButton = { TextButton(onClick = onInstall) { Text(tr("install_plugin")) } },
		dismissButton = { TextButton(onClick = onClose) { Text(tr("close")) } },
	)
}

private fun pluginLabel(plugin: ClientPlugin): String = when (plugin) {
	is ClientPlugin.Language -> "${plugin.label} (${plugin.locale})"
	is ClientPlugin.Theme -> plugin.label
}

private fun kindLabel(plugin: ClientPlugin, tr: (String) -> String): String =
	tr(if (plugin is ClientPlugin.Language) "kind_language_pack" else "kind_theme")

private fun isActive(plugin: ClientPlugin, locale: String, theme: String): Boolean = when (plugin) {
	is ClientPlugin.Language -> plugin.locale == locale
	is ClientPlugin.Theme -> plugin.id == theme
}
