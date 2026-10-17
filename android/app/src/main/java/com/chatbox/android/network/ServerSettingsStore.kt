package com.chatbox.android.network

import android.content.Context

/** Persists the chosen server origin in the `chatbox` preferences (§14). */
class ServerSettingsStore(context: Context) {
	private val prefs = context.getSharedPreferences("chatbox", Context.MODE_PRIVATE)

	fun load(): ServerAddress? {
		val raw = prefs.getString(KEY, null) ?: return null
		return runCatching { ServerAddress.parse(raw) }.getOrNull()
	}

	fun save(address: ServerAddress) {
		prefs.edit().putString(KEY, address.origin).apply()
	}

	fun clear() {
		prefs.edit().remove(KEY).apply()
	}

	private companion object {
		const val KEY = "server_address"
	}
}
