package com.chatbox.android.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.chatbox.android.ui.theme.ChatBoxTheme

/** Application root: shows the login flow or the chat screen. */
@Composable
fun ChatBoxRoot(viewModel: ChatViewModel = viewModel()) {
	val state by viewModel.state.collectAsStateWithLifecycle()
	val dark = when (state.theme) {
		"dark" -> true
		"light" -> false
		else -> isSystemInDarkTheme()
	}
	ChatBoxTheme(darkTheme = dark) {
		if (state.session == null) {
			LoginScreen(state, viewModel::login, viewModel::register)
		} else {
			ChatScreen(state, viewModel)
		}
	}
}
