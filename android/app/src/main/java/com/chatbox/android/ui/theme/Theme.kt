package com.chatbox.android.ui.theme

import android.graphics.Color as AndroidColor
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import com.chatbox.android.plugins.ClientPlugin

@Composable
fun ChatBoxTheme(
	theme: ClientPlugin.Theme? = null,
	darkTheme: Boolean = isSystemInDarkTheme(),
	content: @Composable () -> Unit,
) {
	val base = if (theme?.dark ?: darkTheme) darkColorScheme() else lightColorScheme()
	MaterialTheme(
		colorScheme = theme?.let { base.withPluginColors(it) } ?: base,
		content = content,
	)
}

/** Overlay the color keys provided by an installed theme package. */
private fun ColorScheme.withPluginColors(theme: ClientPlugin.Theme): ColorScheme {
	fun pick(key: String, fallback: Color): Color {
		val value = theme.colors[key] ?: return fallback
		return runCatching { Color(AndroidColor.parseColor(value)) }.getOrDefault(fallback)
	}
	return copy(
		primary = pick("primary", primary),
		onPrimary = pick("onPrimary", onPrimary),
		primaryContainer = pick("primarySoft", primaryContainer),
		secondary = pick("bubbleOut", secondary),
		onSecondary = pick("bubbleOutText", onSecondary),
		background = pick("background", background),
		onBackground = pick("text", onBackground),
		surface = pick("surface", surface),
		onSurface = pick("text", onSurface),
		surfaceVariant = pick("surfaceAlt", surfaceVariant),
		onSurfaceVariant = pick("muted", onSurfaceVariant),
		outline = pick("border", outline),
	)
}
