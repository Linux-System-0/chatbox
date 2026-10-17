package com.chatbox.android.data

import android.content.Context
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/** Persists the signed-in session locally. */
class SessionStore(context: Context) {
	private val prefs = context.getSharedPreferences("chatbox", Context.MODE_PRIVATE)

	fun load(): Session? {
		val raw = prefs.getString(KEY, null) ?: return null
		return runCatching { Json.decodeFromString<Session>(raw) }.getOrNull()
	}

	fun save(session: Session) {
		prefs.edit().putString(KEY, Json.encodeToString(session)).apply()
	}

	fun clear() {
		prefs.edit().remove(KEY).apply()
	}

	private companion object {
		const val KEY = "session"
	}
}
