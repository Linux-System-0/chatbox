package com.chatbox.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
import com.chatbox.android.network.CertificateInfo
import com.chatbox.android.network.ServerAddress

/** First-connection confirmation and fingerprint-change warning (§9). */
@Composable
fun FingerprintDialog(
	prompt: FingerprintPrompt,
	onConfirm: () -> Unit,
	onCancel: () -> Unit,
	tr: (String) -> String,
) {
	AlertDialog(
		// Outside taps and the back gesture must not skip the confirmation.
		onDismissRequest = {},
		title = {
			Text(tr(if (prompt.changed) "certificate_changed_title" else "certificate_confirm_title"))
		},
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
				Text(
					tr(if (prompt.changed) "certificate_changed_body" else "certificate_confirm_body"),
					style = MaterialTheme.typography.bodyMedium,
				)
				prompt.expected?.let { expected ->
					Text(
						tr("certificate_expected_fingerprint").replace("{fingerprint}", expected.uppercase()),
						style = MaterialTheme.typography.bodySmall,
					)
				}
				Text(prompt.address.origin, style = MaterialTheme.typography.titleSmall)
				CertificateDetails(prompt.info, tr)
			}
		},
		confirmButton = {
			TextButton(onClick = onConfirm) { Text(tr("certificate_confirm")) }
		},
		dismissButton = {
			TextButton(onClick = onCancel) { Text(tr("cancel")) }
		},
	)
}

/** Trust failure raised outside the server-setup probe, for example during login (§9). */
@Composable
fun CertificateTrustDialog(
	info: CertificateInfo,
	onTrust: () -> Unit,
	onCancel: () -> Unit,
	tr: (String) -> String,
) {
	AlertDialog(
		onDismissRequest = onCancel,
		title = { Text(tr("certificate_trust_title")) },
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
				Text(tr("certificate_trust_failed"), style = MaterialTheme.typography.bodyMedium)
				CertificateDetails(info, tr)
			}
		},
		confirmButton = {
			TextButton(onClick = onTrust) {
				Text(tr("trust_this_certificate"), color = MaterialTheme.colorScheme.error)
			}
		},
		dismissButton = {
			TextButton(onClick = onCancel) { Text(tr("cancel")) }
		},
	)
}

/** One-time cleartext consent for a host the user is about to use (§10). */
@Composable
fun CleartextConsentDialog(
	address: ServerAddress,
	onConfirm: () -> Unit,
	onCancel: () -> Unit,
	tr: (String) -> String,
) {
	AlertDialog(
		onDismissRequest = onCancel,
		title = { Text(tr("cleartext_title")) },
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
				Text(tr("cleartext_body"), style = MaterialTheme.typography.bodyMedium)
				Text(address.origin, style = MaterialTheme.typography.titleSmall)
			}
		},
		confirmButton = {
			TextButton(onClick = onConfirm) {
				Text(tr("cleartext_confirm"), color = MaterialTheme.colorScheme.error)
			}
		},
		dismissButton = {
			TextButton(onClick = onCancel) { Text(tr("cancel")) }
		},
	)
}
