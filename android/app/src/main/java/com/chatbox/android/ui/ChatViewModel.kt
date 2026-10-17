package com.chatbox.android.ui

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.chatbox.android.BuildConfig
import com.chatbox.android.data.ApiClient
import com.chatbox.android.data.AuthResponse
import com.chatbox.android.data.ConversationView
import com.chatbox.android.data.LoginRequest
import com.chatbox.android.data.Message
import com.chatbox.android.data.ReadRequest
import com.chatbox.android.data.RegisterRequest
import com.chatbox.android.data.RefreshRequest
import com.chatbox.android.data.Session
import com.chatbox.android.data.SessionStore
import com.chatbox.android.realtime.ChatSocket
import com.chatbox.android.realtime.ServerFrame
import com.chatbox.android.plugins.ClientPlugin
import com.chatbox.android.plugins.ClientPluginStore
import com.chatbox.android.plugins.ClientPlugins
import com.chatbox.android.plugins.MAX_CLIENT_PLUGIN_BYTES
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import retrofit2.HttpException
import java.io.ByteArrayOutputStream
import java.io.InputStream

data class ChatUiState(
	val session: Session? = null,
	val conversations: List<ConversationView> = emptyList(),
	val messages: List<Message> = emptyList(),
	val selectedId: Long? = null,
	val status: String = "offline",
	val locale: String = "en",
	val theme: String = "system",
	val plugins: List<ClientPlugin> = emptyList(),
	val error: String? = null,
	val busy: Boolean = false,
)

class ChatViewModel(application: Application) : AndroidViewModel(application) {
	private val api = ApiClient.create(BuildConfig.API_BASE_URL)
	private val store = SessionStore(application)
	private val pluginStore = ClientPluginStore(application)
	private val _state = MutableStateFlow(ChatUiState())
	val state: StateFlow<ChatUiState> = _state.asStateFlow()
	private var socket: ChatSocket? = null

	init {
		store.load()?.let { session ->
			_state.update { it.copy(session = session) }
			applyPlugins(session)
			connect(session)
			refresh()
		}
	}

	fun login(userId: String, password: String) = viewModelScope.launch {
		_state.update { it.copy(busy = true, error = null) }
		runCatching { api.login(LoginRequest(userId = userId, password = password)) }
			.onSuccess { authenticate(it) }
			.onFailure { fail(it) }
		_state.update { it.copy(busy = false) }
	}

	fun register(userId: String, password: String, displayName: String?) = viewModelScope.launch {
		_state.update { it.copy(busy = true, error = null) }
		runCatching {
			api.register(
				RegisterRequest(
					userId = userId,
					password = password,
					displayName = displayName?.takeIf { it.isNotBlank() },
				),
			)
		}
			.onSuccess { authenticate(it) }
			.onFailure { fail(it) }
		_state.update { it.copy(busy = false) }
	}

	private fun authenticate(response: AuthResponse) {
		val session = Session(
			user = response.user,
			accessToken = response.tokens.accessToken,
			refreshToken = response.tokens.refreshToken,
			sessionId = response.tokens.sessionId,
			deviceId = response.tokens.deviceId,
		)
		store.save(session)
		_state.update { it.copy(session = session) }
		applyPlugins(session)
		connect(session)
		refresh()
	}

	private fun connect(session: Session) {
		socket?.close()
		socket = ChatSocket(BuildConfig.API_BASE_URL, session, ::onFrame, { status ->
			_state.update { it.copy(status = status) }
		}).also { it.connect() }
	}

	private fun onFrame(frame: ServerFrame) {
		when (frame) {
			is ServerFrame.Incoming -> {
				val message = frame.message
				_state.update { current ->
					val messages =
						if (current.selectedId == message.conversationId) current.messages + message
						else current.messages
					val conversations = current.conversations.map {
						if (it.id == message.conversationId) it.copy(lastMessage = message) else it
					}
					current.copy(messages = messages, conversations = conversations)
				}
				socket?.acknowledge(message.conversationId, message.seq)
			}
			is ServerFrame.Hint -> select(frame.conversationId)
			is ServerFrame.Failure -> _state.update { it.copy(error = frame.message) }
			else -> Unit
		}
	}

	fun refresh() = viewModelScope.launch {
		runCatching { withAuth { api.conversations(it) } }
			.onSuccess { response -> _state.update { it.copy(conversations = response.conversations) } }
			.onFailure { fail(it) }
	}

	fun select(conversationId: Long) = viewModelScope.launch {
		_state.update { it.copy(selectedId = conversationId, messages = emptyList()) }
		runCatching { withAuth { api.messages(it, conversationId) } }
			.onSuccess { response ->
				_state.update { it.copy(messages = response.messages) }
				response.messages.lastOrNull()?.let { last ->
					runCatching { withAuth { api.markRead(it, conversationId, ReadRequest(last.seq)) } }
				}
			}
			.onFailure { fail(it) }
	}

	fun send(text: String) {
		val id = _state.value.selectedId ?: return
		if (text.isBlank()) return
		socket?.sendText(id, text.trim())
	}

	fun closeConversation() = _state.update { it.copy(selectedId = null, messages = emptyList()) }

	fun startDirect(userId: String) = viewModelScope.launch {
		runCatching { withAuth { api.createDirect(it, com.chatbox.android.data.DirectRequest(userId)) } }
			.onSuccess { response -> refresh(); select(response.conversation.id) }
			.onFailure { fail(it) }
	}

	fun addContact(userId: String) = viewModelScope.launch {
		runCatching { withAuth { api.addContact(it, com.chatbox.android.data.DirectRequest(userId)) } }
			.onFailure { fail(it) }
	}

	fun logout() {
		socket?.close()
		socket = null
		store.clear()
		_state.value = ChatUiState()
	}

	fun setLocale(locale: String) {
		_state.update { it.copy(locale = locale) }
		_state.value.session?.let { pluginStore.setActiveLocale(it.user.userId, locale) }
	}

	fun setTheme(theme: String) {
		_state.update { it.copy(theme = theme) }
		_state.value.session?.let { pluginStore.setActiveTheme(it.user.userId, theme) }
	}

	/** Install a plugin file chosen with the system file picker. */
	fun installPlugin(uri: Uri) = viewModelScope.launch {
		val session = _state.value.session ?: return@launch
		_state.update { it.copy(busy = true, error = null) }
		runCatching {
			withContext(Dispatchers.IO) {
				val context = getApplication<Application>()
				val text = context.contentResolver.openInputStream(uri)?.use { input ->
					input.readLimited(MAX_CLIENT_PLUGIN_BYTES).toString(Charsets.UTF_8)
				} ?: error("cannot read plugin file")
				val plugin = pluginStore.install(session.user.userId, text)
				when (plugin) {
					is ClientPlugin.Language -> pluginStore.setActiveLocale(session.user.userId, plugin.locale)
					is ClientPlugin.Theme -> pluginStore.setActiveTheme(session.user.userId, plugin.id)
				}
				pluginStore.list(session.user.userId) to plugin
			}
		}.onSuccess { (plugins, plugin) ->
			if (_state.value.session?.user?.userId != session.user.userId) return@onSuccess
			val current = _state.value
			val locale = current.locale.takeIf { code ->
				code == ClientPlugins.DEFAULT_LOCALE || plugins.any { it is ClientPlugin.Language && it.locale == code }
			} ?: ClientPlugins.DEFAULT_LOCALE
			val theme = current.theme.takeIf { id ->
				id in ClientPlugins.BASE_THEMES || plugins.any { it is ClientPlugin.Theme && it.id == id }
			} ?: ClientPlugins.DEFAULT_THEME
			val nextLocale = if (plugin is ClientPlugin.Language) plugin.locale else locale
			val nextTheme = if (plugin is ClientPlugin.Theme) plugin.id else theme
			pluginStore.setActiveLocale(session.user.userId, nextLocale)
			pluginStore.setActiveTheme(session.user.userId, nextTheme)
			_state.update {
				it.copy(plugins = plugins, locale = nextLocale, theme = nextTheme)
			}
		}.onFailure { error ->
			_state.update {
				it.copy(
					error = "${ClientPlugins.translate(it.plugins, it.locale, "plugin_error")}: ${error.message ?: "unknown error"}",
				)
			}
		}
		_state.update { it.copy(busy = false) }
	}

	/** Remove one plugin installed for the current account. */
	fun removePlugin(name: String) = viewModelScope.launch {
		val session = _state.value.session ?: return@launch
		runCatching {
			withContext(Dispatchers.IO) {
				pluginStore.remove(session.user.userId, name)
				pluginStore.list(session.user.userId)
			}
		}.onSuccess { plugins ->
			if (_state.value.session?.user?.userId != session.user.userId) return@onSuccess
			val locale = _state.value.locale.takeIf { code ->
				code == ClientPlugins.DEFAULT_LOCALE || plugins.any { it is ClientPlugin.Language && it.locale == code }
			} ?: ClientPlugins.DEFAULT_LOCALE
			val theme = _state.value.theme.takeIf { id ->
				id in ClientPlugins.BASE_THEMES || plugins.any { it is ClientPlugin.Theme && it.id == id }
			} ?: ClientPlugins.DEFAULT_THEME
			pluginStore.setActiveLocale(session.user.userId, locale)
			pluginStore.setActiveTheme(session.user.userId, theme)
			_state.update { it.copy(plugins = plugins, locale = locale, theme = theme) }
		}.onFailure { fail(it) }
	}

	private fun applyPlugins(session: Session) {
		val userId = session.user.userId
		val plugins = pluginStore.list(userId)
		val locale = pluginStore.activeLocale(userId)?.takeIf { code ->
			code == ClientPlugins.DEFAULT_LOCALE || plugins.any { it is ClientPlugin.Language && it.locale == code }
		} ?: ClientPlugins.DEFAULT_LOCALE
		val theme = pluginStore.activeTheme(userId)?.takeIf { id ->
			id in ClientPlugins.BASE_THEMES || plugins.any { it is ClientPlugin.Theme && it.id == id }
		} ?: ClientPlugins.DEFAULT_THEME
		pluginStore.setActiveLocale(userId, locale)
		pluginStore.setActiveTheme(userId, theme)
		_state.update { it.copy(plugins = plugins, locale = locale, theme = theme) }
	}

	fun dismissError() = _state.update { it.copy(error = null) }

	/**
	 * Run an authenticated call, refreshing the access token once on a 401 so a
	 * long-lived session survives access-token expiry.
	 */
	private suspend fun <T> withAuth(block: suspend (String) -> T): T {
		val session = _state.value.session ?: error("not signed in")
		return try {
			block("Bearer ${session.accessToken}")
		} catch (e: HttpException) {
			if (e.code() != 401) throw e
			val refreshed = api.refresh(RefreshRequest(session.refreshToken))
			val next = session.copy(
				accessToken = refreshed.tokens.accessToken,
				refreshToken = refreshed.tokens.refreshToken,
				sessionId = refreshed.tokens.sessionId,
				deviceId = refreshed.tokens.deviceId,
			)
			store.save(next)
			_state.update { it.copy(session = next) }
			block("Bearer ${next.accessToken}")
		}
	}

	private fun fail(error: Throwable) {
		val message = when (error) {
			is HttpException -> "request failed (${error.code()})"
			else -> error.message ?: "request failed"
		}
		_state.update { it.copy(error = message) }
	}

	private fun InputStream.readLimited(max: Int): ByteArray {
		val output = ByteArrayOutputStream()
		val buffer = ByteArray(8192)
		var total = 0
		while (true) {
			val read = read(buffer)
			if (read < 0) break
			total += read
			require(total <= max) { "plugin file is too large (max 256 KiB)" }
			output.write(buffer, 0, read)
		}
		return output.toByteArray()
	}

	override fun onCleared() {
		socket?.close()
		super.onCleared()
	}
}
