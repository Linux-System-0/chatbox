package com.chatbox.android.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.chatbox.android.plugins.ClientPlugin
import com.chatbox.android.plugins.ClientPlugins
import com.chatbox.android.ui.theme.ChatBoxTheme

/** Application root: server setup, login, or chat, plus the global dialogs. */
@Composable
fun ChatBoxRoot(viewModel: ChatViewModel = viewModel()) {
	val state by viewModel.state.collectAsStateWithLifecycle()
	val activeTheme = state.plugins
		.filterIsInstance<ClientPlugin.Theme>()
		.firstOrNull { it.id == state.theme }
	val dark = activeTheme?.dark ?: when (state.theme) {
		"dark" -> true
		"light" -> false
		else -> isSystemInDarkTheme()
	}
	val tr = { key: String -> ClientPlugins.translate(state.plugins, state.locale, key) }
	var showNetworkSettings by remember { mutableStateOf(false) }
	val importLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
		uri?.let(viewModel::importCustomCertificate)
	}

	ChatBoxTheme(theme = activeTheme, darkTheme = dark) {
		if (showNetworkSettings) {
			NetworkSettingsScreen(
				state = state,
				onTrustSystemCAs = viewModel::setTrustSystemCAs,
				onSetVerificationDisabled = viewModel::setVerificationDisabled,
				onAddAllowlistEntry = viewModel::addAllowlistEntry,
				onRemoveAllowlistEntry = viewModel::removeAllowlistEntry,
				onImportCa = { importLauncher.launch(arrayOf("*/*")) },
				onRemoveCa = viewModel::removeCustomCertificate,
				onClose = { showNetworkSettings = false },
				tr = tr,
			)
		} else {
			when {
				state.server == null -> ServerSetupScreen(
					state = state,
					onConnect = viewModel::submitServer,
					onOpenNetworkSettings = { showNetworkSettings = true },
					onRetry = viewModel::retryProbe,
					onTrustCertificate = viewModel::trustPresentedCertificate,
					onDismissFailure = viewModel::dismissProbeFailure,
					tr = tr,
				)

				state.session == null -> LoginScreen(
					state = state,
					onLogin = viewModel::login,
					onRegister = viewModel::register,
					onOpenNetworkSettings = { showNetworkSettings = true },
					onChangeServer = viewModel::changeServer,
					tr = tr,
				)

				else -> ChatScreen(
					state = state,
					viewModel = viewModel,
					onOpenNetworkSettings = { showNetworkSettings = true },
				)
			}
		}

		state.fingerprintPrompt?.let { prompt ->
			FingerprintDialog(
				prompt = prompt,
				onConfirm = viewModel::confirmFingerprint,
				onCancel = viewModel::cancelFingerprint,
				tr = tr,
			)
		}
		state.trustPrompt?.let { info ->
			CertificateTrustDialog(
				info = info,
				onTrust = viewModel::confirmTrustPrompt,
				onCancel = viewModel::dismissTrustPrompt,
				tr = tr,
			)
		}
		state.cleartextPrompt?.let { address ->
			CleartextConsentDialog(
				address = address,
				onConfirm = viewModel::confirmCleartextConsent,
				onCancel = viewModel::cancelCleartextConsent,
				tr = tr,
			)
		}
	}
}
