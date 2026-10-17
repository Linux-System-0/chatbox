package com.chatbox.android.plugins

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement

/** Client plugin API version understood by this app. */
const val CLIENT_PLUGIN_API_VERSION = 1

/** Maximum size of one plugin file. */
const val MAX_CLIENT_PLUGIN_BYTES = 256 * 1024

@Serializable
private data class PluginFile(
	val name: String,
	val version: String,
	val kind: String,
	@SerialName("api_version") val apiVersion: Int,
	val description: String = "",
	val payload: JsonObject,
)

@Serializable
data class LanguagePayload(
	val locale: String,
	val label: String,
	val strings: Map<String, String> = emptyMap(),
)

@Serializable
data class ThemePayload(
	val id: String,
	val label: String,
	val dark: Boolean = false,
	val colors: Map<String, String> = emptyMap(),
)

/** A validated client plugin installed for one account on this device. */
sealed interface ClientPlugin {
	val name: String
	val version: String
	val description: String

	data class Language(
		override val name: String,
		override val version: String,
		override val description: String,
		val locale: String,
		val label: String,
		val strings: Map<String, String>,
	) : ClientPlugin

	data class Theme(
		override val name: String,
		override val version: String,
		override val description: String,
		val id: String,
		val label: String,
		val dark: Boolean,
		val colors: Map<String, String>,
	) : ClientPlugin
}

/** Parses and validates the shared JSON client-plugin format. */
object ClientPluginParser {
	private val json = Json { ignoreUnknownKeys = true }
	private val namePattern = Regex("^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")
	private val localePattern = Regex("^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$")
	const colorPattern = Regex("^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$")

	val themeColorKeys: Set<String> = setOf(
		"background",
		"surface",
		"surfaceAlt",
		"text",
		"muted",
		"border",
		"primary",
		"primaryHover",
		"onPrimary",
		"primarySoft",
		"bubbleIn",
		"bubbleOut",
		"bubbleOutText",
	)

	fun parse(text: String): ClientPlugin {
		require(text.toByteArray(Charsets.UTF_8).size <= MAX_CLIENT_PLUGIN_BYTES) {
			"plugin file is too large (max 256 KiB)"
		}
		val file = runCatching { json.decodeFromString<PluginFile>(text) }
			.getOrElse { throw IllegalArgumentException("plugin file is not a valid ChatBox client plugin: ${it.message}") }

		require(namePattern.matches(file.name)) { "plugin name is invalid" }
		require(file.version.isNotBlank()) { "plugin version is required" }
		require(file.apiVersion == CLIENT_PLUGIN_API_VERSION) {
			"unsupported api_version ${file.apiVersion} (expected $CLIENT_PLUGIN_API_VERSION)"
		}

		return when (file.kind) {
			"language-pack" -> language(file)
			"theme" -> theme(file)
			else -> throw IllegalArgumentException("unsupported plugin kind '${file.kind}'")
		}
	}

	private fun language(file: PluginFile): ClientPlugin.Language {
		val payload = runCatching { json.decodeFromJsonElement<LanguagePayload>(file.payload) }
			.getOrElse { throw IllegalArgumentException("invalid language-pack payload: ${it.message}") }
		require(localePattern.matches(payload.locale)) { "payload.locale is invalid" }
		require(payload.label.isNotBlank()) { "payload.label is required" }
		require(payload.strings.isNotEmpty()) { "payload.strings must not be empty" }
		require(payload.strings.size <= 2000) { "payload.strings has too many entries" }
		payload.strings.forEach { (key, value) ->
			require(key.isNotBlank()) { "payload.strings keys must not be empty" }
			require(value.length <= 4000) { "payload.strings.$key is too long" }
		}
		return ClientPlugin.Language(
			name = file.name,
			version = file.version,
			description = file.description,
			locale = payload.locale,
			label = payload.label,
			strings = payload.strings,
		)
	}

	private fun theme(file: PluginFile): ClientPlugin.Theme {
		val payload = runCatching { json.decodeFromJsonElement<ThemePayload>(file.payload) }
			.getOrElse { throw IllegalArgumentException("invalid theme payload: ${it.message}") }
		require(namePattern.matches(payload.id)) { "payload.id is invalid" }
		require(payload.id !in ClientPlugins.BASE_THEMES) { "payload.id is reserved" }
		require(payload.label.isNotBlank()) { "payload.label is required" }
		val colors = payload.colors.filterKeys { it in themeColorKeys }
		require(colors.isNotEmpty()) { "payload.colors must contain at least one known color" }
		colors.forEach { (key, value) ->
			require(colorPattern.matches(value)) { "payload.colors.$key must be a hex color" }
		}
		return ClientPlugin.Theme(
			name = file.name,
			version = file.version,
			description = file.description,
			id = payload.id,
			label = payload.label,
			dark = payload.dark,
			colors = colors,
		)
	}
}
