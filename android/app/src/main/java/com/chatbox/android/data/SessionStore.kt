package com.chatbox.android.data

import android.content.Context
import com.chatbox.android.network.ServerAddress
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/** Persists the signed-in session locally, bound to the server origin. */
class SessionStore(context: Context) {
	private val prefs = context.getSharedPreferences("chatbox", Context.MODE_PRIVATE)

	/**
	 * Load the stored session only when it belongs to [origin]. Sessions that
	 * predate origin binding cannot be decoded and are discarded once.
	 */
	fun load(origin: ServerAddress): Session? {
		val raw = prefs.getString(KEY, null) ?: return null
		val session = runCatching { Json.decodeFromString<Session>(raw) }.getOrNull()
		if (session == null || session.origin != origin.origin) {
			clear()
			return null
		}
		return session
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
