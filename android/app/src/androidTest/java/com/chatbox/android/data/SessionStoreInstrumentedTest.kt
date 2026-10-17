package com.chatbox.android.data

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.chatbox.android.network.ServerAddress
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Session/origin binding tests (docs/android-network-security.md §13): a stored
 * session is usable only for the origin it was issued for, and sessions written
 * before origin binding are discarded once.
 */
@RunWith(AndroidJUnit4::class)
class SessionStoreInstrumentedTest {

	private lateinit var context: Context

	@Before
	fun setUp() {
		context = ApplicationProvider.getApplicationContext()
		context.getSharedPreferences("chatbox", Context.MODE_PRIVATE).edit().clear().commit()
	}

	@After
	fun tearDown() {
		context.getSharedPreferences("chatbox", Context.MODE_PRIVATE).edit().clear().commit()
	}

	private fun session(origin: String) = Session(
		user = User(id = 1, userId = "alice", displayName = "Alice"),
		accessToken = "access",
		refreshToken = "refresh",
		sessionId = "session",
		deviceId = 1,
		origin = origin,
	)

	@Test
	fun loadsSessionOnlyForItsOrigin() {
		val store = SessionStore(context)
		val stored = session("https://example.com")
		store.save(stored)

		assertEquals(stored, store.load(ServerAddress.parse("https://example.com")))
	}

	@Test
	fun discardsSessionForAnotherOrigin() {
		val store = SessionStore(context)
		store.save(session("https://example.com"))

		assertNull(store.load(ServerAddress.parse("https://other.example")))
		// The mismatched session was cleared, not merely ignored.
		assertNull(store.load(ServerAddress.parse("https://example.com")))
	}

	@Test
	fun discardsOriginLessSessionsOnce() {
		val legacy = """
			{
				"user": {"id": 1, "user_id": "alice", "display_name": "Alice"},
				"accessToken": "access",
				"refreshToken": "refresh",
				"sessionId": "session",
				"deviceId": 1
			}
		""".trimIndent()
		context.getSharedPreferences("chatbox", Context.MODE_PRIVATE).edit()
			.putString("session", legacy)
			.commit()

		val store = SessionStore(context)
		assertNull(store.load(ServerAddress.parse("https://example.com")))
		assertNull(
			context.getSharedPreferences("chatbox", Context.MODE_PRIVATE)
				.getString("session", null),
		)
	}
}
