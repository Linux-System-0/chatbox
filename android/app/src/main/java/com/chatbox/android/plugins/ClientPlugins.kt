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
		"online" to "Online",
		"offline" to "Offline",
		"connecting" to "Connecting…",
		"new_chat" to "New chat (user id)",
		"language" to "Language",
		"theme" to "Theme",
		"time_settings" to "Time display",
		"time_zone" to "Time zone",
		"time_zone_system" to "System time zone",
		"time_zone_hint" to "Choose the system zone or a fixed offset from UTC.",
		"time_format" to "Time format",
		"time_format_system" to "System default",
		"time_format_custom" to "Custom pattern",
		"time_pattern" to "Pattern",
		"time_format_hint" to "Tokens: YYYY, MM, DD, HH, mm, ss, Z. Wrap literal text in [brackets].",
		"time_preview" to "Preview",
		"reset_defaults" to "Reset to defaults",
		"read" to "Read",
		"unread" to "Not read yet",
		"read_by_count" to "Read by {read}/{total}",
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
		"server" to "Server",
		"server_setup_title" to "Choose a server",
		"server_setup_hint" to "Enter the address of your ChatBox backend and choose http or https.",
		"server_address" to "Server address",
		"server_address_placeholder" to "host or host:port",
		"scheme_http" to "http",
		"scheme_https" to "https",
		"connect" to "Connect",
		"connecting_server" to "Connecting…",
		"change_server" to "Change server",
		"network_settings" to "Network settings",
		"trust_system_cas" to "Trust system CAs",
		"trust_system_cas_hint" to "System certificate authorities are accepted in addition to custom CAs.",
		"allow_cleartext" to "Allow cleartext",
		"allow_cleartext_hint" to "Plain HTTP and ws:// are allowed only for these hosts. Ports are ignored.",
		"allowlist_kind_dns" to "DNS pattern",
		"allowlist_kind_ip" to "IP address",
		"allowlist_kind_cidr" to "CIDR block",
		"allowlist_add" to "Add",
		"allowlist_empty" to "No cleartext hosts are allowed.",
		"custom_cas" to "Custom CAs",
		"custom_cas_hint" to "Import PEM or DER certificates. Only public certificates are stored.",
		"import_ca" to "Import CA",
		"no_custom_cas" to "No custom CAs are imported.",
		"disable_verification" to "Disable security verification",
		"disable_verification_hint" to "No certificate check, no hostname check, and no cleartext restriction.",
		"verification_disabled_title" to "Disable security verification?",
		"verification_disabled_body" to "Certificate and hostname checks and cleartext restrictions will be disabled. Passwords, tokens, and messages can be read or modified by anyone on the network. Use this only for debugging.",
		"verification_disabled_confirm" to "Disable verification",
		"verification_disabled_banner" to "Security verification is disabled since {time}",
		"certificate_fingerprint" to "SHA-256 fingerprint",
		"certificate_subject" to "Subject",
		"certificate_issuer" to "Issuer",
		"certificate_validity" to "Valid from {from} to {to}",
		"certificate_confirm_title" to "Confirm the server certificate",
		"certificate_confirm_body" to "This is the first connection to this server. Compare the fingerprint with the one shown by your server administrator.",
		"certificate_changed_title" to "The server certificate changed",
		"certificate_changed_body" to "The certificate no longer matches the confirmed fingerprint. This can be a normal certificate rotation, or someone may be intercepting the connection. Confirm only after checking with the server administrator.",
		"certificate_expected_fingerprint" to "Previously confirmed: {fingerprint}",
		"certificate_confirm" to "Trust and continue",
		"certificate_trust_title" to "Certificate not trusted",
		"certificate_trust_failed" to "The certificate chain is not trusted.",
		"cancel" to "Cancel",
		"retry" to "Retry",
		"cleartext_title" to "Allow cleartext traffic?",
		"cleartext_body" to "Credentials, tokens, and message content will travel unencrypted over the network. WebSocket traffic (ws://) inherits the same exposure.",
		"cleartext_confirm" to "Allow cleartext",
		"probe_failure_title" to "Cannot reach the server",
		"probe_invalid_input" to "The server address is not valid.",
		"probe_dns" to "The host name could not be resolved.",
		"probe_connection_refused" to "The connection was refused. Is the server running?",
		"probe_timeout" to "The connection timed out.",
		"probe_tls" to "The TLS handshake failed.",
		"probe_certificate" to "The server certificate is not trusted.",
		"probe_cleartext" to "Cleartext traffic to this host is not allowed.",
		"probe_status" to "The server returned an unexpected response.",
		"probe_unknown" to "The server could not be reached.",
		"open_network_settings" to "Open network settings",
		"trust_this_certificate" to "Trust this certificate",
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
