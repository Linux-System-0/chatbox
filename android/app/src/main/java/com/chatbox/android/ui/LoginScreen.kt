package com.chatbox.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
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
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import com.chatbox.android.plugins.ClientPlugins

@Composable
fun LoginScreen(
	state: ChatUiState,
	onLogin: (String, String) -> Unit,
	onRegister: (String, String, String?) -> Unit,
) {
	var mode by remember { mutableStateOf("login") }
	var userId by remember { mutableStateOf("") }
	var password by remember { mutableStateOf("") }
	var displayName by remember { mutableStateOf("") }
	val tr = { key: String -> ClientPlugins.translate(state.locale, key) }

	Column(
		modifier = Modifier.fillMaxSize().padding(24.dp),
		verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
		horizontalAlignment = Alignment.CenterHorizontally,
	) {
		Text(tr("app_name"), style = MaterialTheme.typography.headlineMedium)
		OutlinedTextField(
			value = userId,
			onValueChange = { userId = it },
			label = { Text(tr("user_id")) },
			singleLine = true,
			modifier = Modifier.fillMaxWidth(),
		)
		if (mode == "register") {
			OutlinedTextField(
				value = displayName,
				onValueChange = { displayName = it },
				label = { Text(tr("display_name")) },
				singleLine = true,
				modifier = Modifier.fillMaxWidth(),
			)
		}
		OutlinedTextField(
			value = password,
			onValueChange = { password = it },
			label = { Text(tr("password")) },
			singleLine = true,
			visualTransformation = PasswordVisualTransformation(),
			modifier = Modifier.fillMaxWidth(),
		)
		Button(
			onClick = {
				if (mode == "login") onLogin(userId, password)
				else onRegister(userId, password, displayName)
			},
			enabled = !state.busy && userId.isNotBlank() && password.isNotBlank(),
			modifier = Modifier.fillMaxWidth(),
		) {
			Text(tr(if (mode == "login") "login" else "register"))
		}
		TextButton(onClick = { mode = if (mode == "login") "register" else "login" }) {
			Text(if (mode == "login") tr("register") else tr("login"))
		}
		state.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
	}
}
