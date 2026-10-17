package com.chatbox.android.plugins

/**
 * Client plugin facade.
 *
 * Language packs and themes are installed at runtime from a JSON file and
 * scoped to the signed-in account. Only the base English dictionary and the
 * built-in light/dark schemes are compiled into the app (see
 * docs/plugin-system.md).
 */
object ClientPlugins {
	const val DEFAULT_LOCALE = "en"
	const val DEFAULT_THEME = "system"

	val BASE_THEMES: Set<String> = setOf("system", "light", "dark")

	data class LanguageOption(val code: String, val label: String)
	data class ThemeOption(val id: String, val label: String, val builtin: Boolean = false)

	private val baseStrings = mapOf(
		"app_name" to "ChatBox",
		"tagline" to "Fast, real-time conversations on every device.",
		"login" to "Sign in",
		"register" to "Create account",
		"logout" to "Sign out",
		"user_id" to "User ID",
		"password" to "Password",
		"display_name" to "Display name",
		"send" to "Send",
		"message_placeholder" to "Write a message…",
		"conversations" to "Conversations",
		"new_chat" to "New chat (user id)",
		"language" to "Language",
		"theme" to "Theme",
		"light" to "Light",
		"dark" to "Dark",
		"system" to "System",
		"close" to "Close",
		"remove" to "Remove",
		"plugins" to "Plugins",
		"install_plugin" to "Install plugin",
		"installed_plugins" to "Installed plugins",
		"no_plugins" to "No plugins installed yet",
		"plugin_install_hint" to
			"Choose a ChatBox client plugin file (.json). Plugins apply only to the current account.",
		"plugin_file_too_large" to "Plugin file is too large (max 256 KiB)",
		"plugin_installed" to "Plugin installed",
		"plugin_removed" to "Plugin removed",
		"plugin_error" to "Plugin installation failed",
		"kind_language_pack" to "Language pack",
		"kind_theme" to "Theme",
		"active" to "Active",
	)

	fun languageOptions(plugins: List<ClientPlugin>): List<LanguageOption> {
		val options = mutableListOf(LanguageOption(DEFAULT_LOCALE, "English"))
		for (pack in plugins.filterIsInstance<ClientPlugin.Language>()) {
			if (options.any { it.code == pack.locale }) continue
			options += LanguageOption(pack.locale, pack.label)
		}
		return options
	}

	fun themeOptions(plugins: List<ClientPlugin>): List<ThemeOption> {
		val options = mutableListOf(
			ThemeOption("system", "System", builtin = true),
			ThemeOption("light", "Light", builtin = true),
			ThemeOption("dark", "Dark", builtin = true),
		)
		for (pack in plugins.filterIsInstance<ClientPlugin.Theme>()) {
			if (options.any { it.id == pack.id }) continue
			options += ThemeOption(pack.id, pack.label)
		}
		return options
	}

	fun translate(plugins: List<ClientPlugin>, locale: String, key: String): String {
		val pack = plugins.filterIsInstance<ClientPlugin.Language>().firstOrNull { it.locale == locale }
		return pack?.strings?.get(key) ?: baseStrings[key] ?: key
	}
}
