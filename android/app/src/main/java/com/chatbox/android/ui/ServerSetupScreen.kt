package com.chatbox.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.unit.dp
import com.chatbox.android.network.AddressException
import com.chatbox.android.network.ProbeFailure
import com.chatbox.android.network.ProbeFailureKind
import com.chatbox.android.network.ServerAddress

@Composable
fun ServerSetupScreen(
	state: ChatUiState,
	onConnect: (String, String) -> Unit,
	onOpenNetworkSettings: () -> Unit,
	onRetry: () -> Unit,
	onTrustCertificate: () -> Unit,
	onDismissFailure: () -> Unit,
	tr: (String) -> String,
) {
	val initialScheme = if (state.serverPrefill.startsWith("https://", ignoreCase = true)) "https" else "http"
	val initialAddress = when {
		state.serverPrefill.startsWith("https://", ignoreCase = true) ->
			state.serverPrefill.substring("https://".length)

		state.serverPrefill.startsWith("http://", ignoreCase = true) ->
			state.serverPrefill.substring("http://".length)

		else -> state.serverPrefill
	}
	var address by remember(initialAddress) { mutableStateOf(initialAddress) }
	var scheme by remember(initialScheme) { mutableStateOf(initialScheme) }
	val preview = remember(address, scheme) {
		runCatching { ServerAddress.parse(address, schemeOf(scheme)).origin }.getOrNull()
	}
	val previewError = remember(address, scheme) {
		if (address.isBlank()) null else runCatching { ServerAddress.parse(address, schemeOf(scheme)) }.exceptionOrNull()
			?.let { it as? AddressException ?: AddressException(it.message ?: "invalid address") }
	}

	Column(
		modifier = Modifier
			.fillMaxSize()
			.verticalScroll(rememberScrollState())
			.padding(24.dp),
		verticalArrangement = Arrangement.spacedBy(12.dp),
		horizontalAlignment = Alignment.CenterHorizontally,
	) {
		Text(tr("server_setup_title"), style = MaterialTheme.typography.headlineMedium)
		Text(
			tr("server_setup_hint"),
			style = MaterialTheme.typography.bodyMedium,
			modifier = Modifier.fillMaxWidth(),
		)
		Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
			FilterChip(
				selected = scheme == "http",
				onClick = { scheme = "http" },
				label = { Text(tr("scheme_http")) },
			)
			FilterChip(
				selected = scheme == "https",
				onClick = { scheme = "https" },
				label = { Text(tr("scheme_https")) },
			)
		}
		OutlinedTextField(
			value = address,
			onValueChange = { value ->
				// A pasted scheme selects the matching toggle and is stripped, so
				// the field always shows host[:port] (see §5.2).
				when {
					value.startsWith("https://", ignoreCase = true) -> {
						scheme = "https"
						address = value.substring("https://".length)
					}

					value.startsWith("http://", ignoreCase = true) -> {
						scheme = "http"
						address = value.substring("http://".length)
					}

					else -> address = value
				}
			},
			label = { Text(tr("server_address")) },
			placeholder = { Text(tr("server_address_placeholder")) },
			singleLine = true,
			isError = previewError != null,
			modifier = Modifier.fillMaxWidth(),
		)
		previewError?.let { Text(it.message.orEmpty(), color = MaterialTheme.colorScheme.error) }
		if (preview != null) {
			Text(
				preview,
				style = MaterialTheme.typography.bodyMedium,
				fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
			)
		}
		state.serverError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
		if (state.probing) {
			LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
			Text(tr("connecting_server"))
		}
		Button(
			onClick = { onConnect(address, scheme) },
			enabled = !state.probing && address.isNotBlank(),
			modifier = Modifier.fillMaxWidth(),
		) {
			Text(tr("connect"))
		}
		TextButton(onClick = onOpenNetworkSettings) { Text(tr("network_settings")) }

		state.probeFailure?.let { failure ->
			ProbeFailureCard(
				failure = failure,
				onRetry = onRetry,
				onTrustCertificate = onTrustCertificate,
				onOpenNetworkSettings = onOpenNetworkSettings,
				onDismiss = onDismissFailure,
				tr = tr,
			)
		}
	}
}

@Composable
private fun ProbeFailureCard(
	failure: ProbeFailure,
	onRetry: () -> Unit,
	onTrustCertificate: () -> Unit,
	onOpenNetworkSettings: () -> Unit,
	onDismiss: () -> Unit,
	tr: (String) -> String,
) {
	Card(modifier = Modifier.fillMaxWidth()) {
		Column(
			modifier = Modifier.padding(12.dp),
			verticalArrangement = Arrangement.spacedBy(6.dp),
		) {
			Text(tr("probe_failure_title"), style = MaterialTheme.typography.titleMedium)
			Text(probeMessage(failure.kind, tr), style = MaterialTheme.typography.bodyMedium)
			failure.certificate?.let { CertificateDetails(it, tr) }
			Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
				TextButton(onClick = onRetry) { Text(tr("retry")) }
				if (failure.certificate != null &&
					(failure.kind == ProbeFailureKind.CertificateTrust || failure.kind == ProbeFailureKind.TlsHandshake)
				) {
					TextButton(onClick = onTrustCertificate) { Text(tr("trust_this_certificate")) }
				}
				TextButton(onClick = onOpenNetworkSettings) { Text(tr("network_settings")) }
				TextButton(onClick = onDismiss) { Text(tr("cancel")) }
			}
		}
	}
}

private fun schemeOf(scheme: String): ServerAddress.Scheme =
	ServerAddress.Scheme.fromWire(scheme) ?: ServerAddress.Scheme.HTTP

private fun probeMessage(kind: ProbeFailureKind, tr: (String) -> String): String = tr(
	when (kind) {
		ProbeFailureKind.InvalidInput -> "probe_invalid_input"
		ProbeFailureKind.Dns -> "probe_dns"
		ProbeFailureKind.ConnectionRefused -> "probe_connection_refused"
		ProbeFailureKind.Timeout -> "probe_timeout"
		ProbeFailureKind.TlsHandshake -> "probe_tls"
		ProbeFailureKind.CertificateTrust -> "probe_certificate"
		ProbeFailureKind.CleartextBlocked -> "probe_cleartext"
		ProbeFailureKind.HttpStatus -> "probe_status"
		ProbeFailureKind.FingerprintRequired,
		ProbeFailureKind.FingerprintChanged,
		ProbeFailureKind.Unknown,
		-> "probe_unknown"
	},
)
