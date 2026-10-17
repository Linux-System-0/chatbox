package com.chatbox.android.plugins

import android.content.Context
import android.util.Base64
import java.io.File

/**
 * Per-user, app-private client plugin storage.
 *
 * Files live under `files/plugins/<encoded user id>/`; the active language and
 * theme selections are kept in SharedPreferences under the same user key. The
 * server never sees these files.
 */
class ClientPluginStore(context: Context) {
	private val root = File(context.filesDir, "plugins")
	private val prefs = context.getSharedPreferences("chatbox.plugins", Context.MODE_PRIVATE)

	fun list(userId: String): List<ClientPlugin> {
		val dir = userDir(userId)
		if (!dir.isDirectory) return emptyList()
		return dir.listFiles { file -> file.isFile && file.name.endsWith(".json") }
			?.mapNotNull { file ->
				runCatching { ClientPluginParser.parse(file.readText()) }.getOrNull()
			}
			?.sortedBy { it.name }
			?: emptyList()
	}

	/** Validate and store a plugin file, replacing a plugin with the same name. */
	fun install(userId: String, text: String): ClientPlugin {
		val plugin = ClientPluginParser.parse(text)
		val dir = userDir(userId).apply { mkdirs() }
		val target = File(dir, "${plugin.name}.json")
		val temp = File(dir, "${plugin.name}.json.tmp")
		temp.writeText(text)
		if (target.exists()) target.delete()
		if (!temp.renameTo(target)) {
			target.writeText(text)
			temp.delete()
		}
		return plugin
	}

	fun remove(userId: String, name: String) {
		val file = File(userDir(userId), "$name.json")
		if (file.exists()) file.delete()
	}

	fun activeLocale(userId: String): String? = prefs.getString(key(userId, "locale"), null)

	fun setActiveLocale(userId: String, locale: String) {
		prefs.edit().putString(key(userId, "locale"), locale).apply()
	}

	fun activeTheme(userId: String): String? = prefs.getString(key(userId, "theme"), null)

	fun setActiveTheme(userId: String, theme: String) {
		prefs.edit().putString(key(userId, "theme"), theme).apply()
	}

	private fun key(userId: String, name: String): String = "$name:${userKey(userId)}"

	private fun userDir(userId: String): File = File(root, userKey(userId))

	private fun userKey(userId: String): String =
		Base64.encodeToString(
			userId.toByteArray(Charsets.UTF_8),
			Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING,
		)
}
