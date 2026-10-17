package com.chatbox.android.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.chatbox.android.network.CertificateInfo
import java.text.DateFormat
import java.util.Date

/** Subject, issuer, validity, and SHA-256 fingerprint of one certificate. */
@Composable
fun CertificateDetails(
	info: CertificateInfo,
	tr: (String) -> String,
	modifier: Modifier = Modifier,
) {
	val format = DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT)
	Column(modifier = modifier.padding(vertical = 4.dp)) {
		Text("${tr("certificate_subject")}: ${info.subject}", style = MaterialTheme.typography.bodySmall)
		Text("${tr("certificate_issuer")}: ${info.issuer}", style = MaterialTheme.typography.bodySmall)
		Text(
			tr("certificate_validity")
				.replace("{from}", format.format(Date(info.notBefore.time)))
				.replace("{to}", format.format(Date(info.notAfter.time))),
			style = MaterialTheme.typography.bodySmall,
		)
		Text(
			"${tr("certificate_fingerprint")}:",
			style = MaterialTheme.typography.bodySmall,
			modifier = Modifier.padding(top = 4.dp),
		)
		Text(
			info.displayFingerprint,
			style = MaterialTheme.typography.bodySmall,
			fontFamily = FontFamily.Monospace,
		)
	}
}
