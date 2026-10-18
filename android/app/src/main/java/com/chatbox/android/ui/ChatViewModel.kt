package com.chatbox.android.ui

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.chatbox.android.data.ApiClient
import com.chatbox.android.data.AuthResponse
import com.chatbox.android.data.ChatBoxApi
import com.chatbox.android.data.ConversationView
import com.chatbox.android.data.LoginRequest
import com.chatbox.android.data.Message
import com.chatbox.android.data.ReadCursorView
import com.chatbox.android.data.ReadRequest
import com.chatbox.android.data.RefreshRequest
import com.chatbox.android.data.RegisterRequest
import com.chatbox.android.data.Session
import com.chatbox.android.data.SessionStore
import com.chatbox.android.network.AddressException
import com.chatbox.android.network.AllowlistEntry
import com.chatbox.android.network.AllowlistKind
import com.chatbox.android.network.CertificateInfo
import com.chatbox.android.network.Certificates
import com.chatbox.android.network.CleartextBlockedException
import com.chatbox.android.network.FingerprintChangedException
import com.chatbox.android.network.FingerprintRequiredException
import com.chatbox.android.network.HealthProbe
import com.chatbox.android.network.HostMatcher
import com.chatbox.android.network.NetworkClientFactory
import com.chatbox.android.network.NetworkPolicy
import com.chatbox.android.network.NetworkPolicyStore
import com.chatbox.android.network.ProbeFailure
import com.chatbox.android.network.ProbeFailureKind
import com.chatbox.android.network.ProbeOutcome
import com.chatbox.android.network.ServerAddress
import com.chatbox.android.network.ServerSettingsStore
import com.chatbox.android.plugins.ClientPlugin
import com.chatbox.android.plugins.ClientPluginStore
import com.chatbox.android.plugins.ClientPlugins
import com.chatbox.android.plugins.MAX_CLIENT_PLUGIN_BYTES
import com.chatbox.android.realtime.ChatSocket
import com.chatbox.android.realtime.ServerFrame
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.security.cert.CertificateException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import javax.net.ssl.SSLPeerUnverifiedException
import okhttp3.OkHttpClient
import retrofit2.HttpException

const val DEFAULT_SERVER_HINT = "10.0.2.2:8080"

/** A fingerprint confirmation the user must answer. */
data class FingerprintPrompt(
	val address: ServerAddress,
	val info: CertificateInfo,
	val changed: Boolean,
	val expected: String? = null,
)

data class ChatUiState(
	val session: Session? = null,
	val conversations: List<ConversationView> = emptyList(),
	val messages: List<Message> = emptyList(),
	val selectedId: Long? = null,
	val status: String = "offline",
	val locale: String = "en",
	val theme: String = "system",
	val timeSettings: TimeDisplaySettings = TimeDisplaySettings(),
	val plugins: List<ClientPlugin> = emptyList(),
	val error: String? = null,
	val busy: Boolean = false,
	val server: ServerAddress? = null,
	val serverPrefill: String = DEFAULT_SERVER_HINT,
	val policy: NetworkPolicy = NetworkPolicy(),
	val verificationDisabledAt: Long? = null,
	val probing: Boolean = false,
	val probeFailure: ProbeFailure? = null,
	val fingerprintPrompt: FingerprintPrompt? = null,
	val cleartextPrompt: ServerAddress? = null,
	val serverError: String? = null,
	val trustPrompt: CertificateInfo? = null,
)

class ChatViewModel(application: Application) : AndroidViewModel(application) {
	private val serverStore = ServerSettingsStore(application)
	private val sessionStore = SessionStore(application)
	private val policyStore = NetworkPolicyStore(application)
	private val pluginStore = ClientPluginStore(application)
	private val timeStore = TimeSettingsStore(application)
	private val _state = MutableStateFlow(ChatUiState())
	val state: StateFlow<ChatUiState> = _state.asStateFlow()
	private var client: OkHttpClient? = null
	private var api: ChatBoxApi? = null
	private var socket: ChatSocket? = null
	private var capture = com.chatbox.android.network.CertificateCapture()
	private var pendingAddress: ServerAddress? = null

	init {
		val policy = policyStore.policy()
		val server = serverStore.load()
		_state.update {
			it.copy(
				server = server,
				serverPrefill = server?.origin ?: DEFAULT_SERVER_HINT,
				policy = policy,
				verificationDisabledAt = policyStore.verificationDisabledAt(),
			)
		}
		if (server == null) {
			// A session is meaningless without the origin it belongs to.
			sessionStore.clear()
		}
		if (server != null) {
			configureNetwork(server)
			val session = sessionStore.load(server)
			if (session != null) {
				_state.update { it.copy(session = session) }
				applyPlugins(session)
				connect(session)
				refresh()
			}
		}
	}

	// ---------------------------------------------------------------------
	// Server setup
	// ---------------------------------------------------------------------

	/** Validate the address and start the consent/probe flow. */
	fun submitServer(raw: String, scheme: String) {
		val selected = ServerAddress.Scheme.fromWire(scheme)
		if (selected == null) {
			_state.update { it.copy(serverError = "unsupported scheme") }
			return
		}
		val address = try {
			ServerAddress.parse(raw, selected)
		} catch (error: AddressException) {
			_state.update { it.copy(serverError = error.message, probeFailure = null) }
			return
		}
		_state.update {
			it.copy(serverError = null, error = null, probeFailure = null, trustPrompt = null)
		}
		val policy = policyStore.policy()
		if (address.isCleartext && !policy.verificationDisabled && !policy.allowsCleartextHost(address.host)) {
			_state.update { it.copy(cleartextPrompt = address) }
			return
		}
		probeCandidate(address)
	}

	/** The user accepted the cleartext warning; remember the host and continue. */
	fun confirmCleartextConsent() {
		val address = _state.value.cleartextPrompt ?: return
		val host = address.host.removeSurrounding("[", "]")
		val kind = if (HostMatcher.isLiteralHost(host)) AllowlistKind.IP else AllowlistKind.DNS
		runCatching { policyStore.addAllowlistEntry(kind, host) }
			.onSuccess { updatePolicyState() }
			.onFailure { error ->
				_state.update { it.copy(cleartextPrompt = null, serverError = error.message) }
				return
			}
		_state.update { it.copy(cleartextPrompt = null) }
		probeCandidate(address)
	}

	fun cancelCleartextConsent() = _state.update { it.copy(cleartextPrompt = null) }

	fun retryProbe() {
		pendingAddress?.let { probeCandidate(it) }
	}

	fun dismissProbeFailure() = _state.update { it.copy(probeFailure = null) }

	/** Add the certificate from a trust failure to the custom CA list and retry. */
	fun trustPresentedCertificate() {
		val failure = _state.value.probeFailure ?: return
		val info = failure.certificate ?: return
		trustCertificate(info)
		_state.update { it.copy(probeFailure = null) }
		pendingAddress?.let { probeCandidate(it) }
	}

	/** Trust a certificate shown by the global trust prompt. */
	fun confirmTrustPrompt() {
		val info = _state.value.trustPrompt ?: return
		trustCertificate(info)
		_state.update { it.copy(trustPrompt = null) }
		val session = _state.value.session
		if (session != null) {
			connect(session)
			refresh()
		}
	}

	fun dismissTrustPrompt() = _state.update { it.copy(trustPrompt = null) }

	private fun trustCertificate(info: CertificateInfo) {
		policyStore.addCustomCertificate(info.certificate)
		updatePolicyState()
		_state.value.server?.let { configureNetwork(it) }
	}

	fun confirmFingerprint() {
		val prompt = _state.value.fingerprintPrompt ?: return
		policyStore.confirmFingerprint(prompt.address.origin, prompt.info.sha256)
		_state.update { it.copy(fingerprintPrompt = null) }
		val server = _state.value.server
		if (server != null && server.origin == prompt.address.origin) {
			_state.value.session?.let { session ->
				connect(session)
				refresh()
			}
		} else {
			probeCandidate(prompt.address)
		}
	}

	fun cancelFingerprint() = _state.update { it.copy(fingerprintPrompt = null) }

	/** Leave the current server and show the server screen again. */
	fun changeServer() {
		val previous = _state.value.server
		logout()
		serverStore.clear()
		client = null
		api = null
		pendingAddress = null
		_state.update {
			it.copy(
				server = null,
				serverPrefill = previous?.origin ?: DEFAULT_SERVER_HINT,
				probeFailure = null,
				fingerprintPrompt = null,
				cleartextPrompt = null,
				serverError = null,
			)
		}
	}

	private fun probeCandidate(address: ServerAddress) {
		pendingAddress = address
		viewModelScope.launch {
			_state.update { it.copy(probing = true, probeFailure = null, serverError = null, error = null) }
			val current = _state.value.server
			if (current != null && current.origin != address.origin && _state.value.session != null) {
				logout()
			}
			val network = configureNetwork(address)
			val outcome = HealthProbe(network, capture).probe(address.origin)
			_state.update { it.copy(probing = false) }
			when (outcome) {
				is ProbeOutcome.Success -> acceptServer(address)
				is ProbeOutcome.Failure -> {
					val failure = outcome.failure
					when (failure.kind) {
						ProbeFailureKind.FingerprintRequired -> {
							val info = failure.certificate
							if (info == null) {
								_state.update { it.copy(probeFailure = failure) }
							} else {
								_state.update {
									it.copy(
										fingerprintPrompt = FingerprintPrompt(address, info, changed = false),
									)
								}
							}
						}

						ProbeFailureKind.FingerprintChanged -> {
							val info = failure.certificate
							if (info == null) {
								_state.update { it.copy(probeFailure = failure) }
							} else {
								_state.update {
									it.copy(
										fingerprintPrompt = FingerprintPrompt(
											address,
											info,
											changed = true,
											expected = failure.expectedFingerprint,
										),
									)
								}
							}
						}

						else -> _state.update { it.copy(probeFailure = failure) }
					}
				}
			}
		}
	}

	private fun acceptServer(address: ServerAddress) {
		serverStore.save(address)
		_state.update {
			it.copy(
				server = address,
				serverPrefill = address.origin,
				probeFailure = null,
				fingerprintPrompt = null,
				cleartextPrompt = null,
				serverError = null,
			)
		}
		if (_state.value.session == null) {
			val session = sessionStore.load(address)
			if (session != null) {
				_state.update { it.copy(session = session) }
				applyPlugins(session)
				connect(session)
				refresh()
			}
		}
	}

	// ---------------------------------------------------------------------
	// Network settings
	// ---------------------------------------------------------------------

	fun setTrustSystemCAs(enabled: Boolean) {
		policyStore.setTrustSystemCAs(enabled)
		afterPolicyChange()
	}

	fun addAllowlistEntry(kind: AllowlistKind, input: String) {
		runCatching { policyStore.addAllowlistEntry(kind, input) }
			.onSuccess { afterPolicyChange() }
			.onFailure { error -> _state.update { it.copy(error = error.message) } }
	}

	fun removeAllowlistEntry(entry: AllowlistEntry) {
		policyStore.removeAllowlistEntry(entry.serialized)
		afterPolicyChange()
	}

	fun importCustomCertificates(bytes: ByteArray) {
		runCatching {
			val certificates = Certificates.parse(bytes)
			certificates.map { policyStore.addCustomCertificate(it) }
		}.onSuccess { afterPolicyChange() }
			.onFailure { error -> _state.update { it.copy(error = error.message) } }
	}

	/** Read a certificate chosen with the system file picker. */
	fun importCustomCertificate(uri: Uri) = viewModelScope.launch {
		runCatching {
			withContext(Dispatchers.IO) {
				getApplication<Application>().contentResolver.openInputStream(uri)?.use {
					it.readCertificateBytes()
				} ?: error("cannot read the certificate file")
			}
		}.onSuccess { importCustomCertificates(it) }
			.onFailure { error ->
				_state.update { it.copy(error = error.message ?: "cannot read the certificate file") }
			}
	}

	fun removeCustomCertificate(info: CertificateInfo) {
		policyStore.removeCustomCertificate(info.sha256)
		afterPolicyChange()
	}

	fun setVerificationDisabled(disabled: Boolean) {
		policyStore.setVerificationDisabled(disabled)
		afterPolicyChange()
	}

	/**
	 * Re-create the shared client so new connections use the updated policy and
	 * reconnect the WebSocket when a session is active.
	 */
	private fun afterPolicyChange() {
		updatePolicyState()
		val server = _state.value.server ?: return
		configureNetwork(server)
		_state.value.session?.let { connect(it) }
	}

	private fun updatePolicyState() {
		_state.update {
			it.copy(
				policy = policyStore.policy(),
				verificationDisabledAt = policyStore.verificationDisabledAt(),
			)
		}
	}

	private fun configureNetwork(address: ServerAddress): OkHttpClient {
		val factory = NetworkClientFactory(policyStore.policy(), address.origin, policyStore)
		val network = factory.create()
		client = network
		api = ApiClient.create(address.origin, network)
		capture = factory.capture
		_state.update {
			it.copy(
				policy = factory.policy,
				verificationDisabledAt = policyStore.verificationDisabledAt(),
			)
		}
		return network
	}

	// ---------------------------------------------------------------------
	// Authentication and chat
	// ---------------------------------------------------------------------

	fun login(userId: String, password: String) = viewModelScope.launch {
		_state.update { it.copy(busy = true, error = null) }
		runCatching { requireApi().login(LoginRequest(userId = userId, password = password)) }
			.onSuccess { authenticate(it) }
			.onFailure { fail(it) }
		_state.update { it.copy(busy = false) }
	}

	fun register(userId: String, password: String, displayName: String?) = viewModelScope.launch {
		_state.update { it.copy(busy = true, error = null) }
		runCatching {
			requireApi().register(
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
		val server = _state.value.server ?: return
		val session = Session(
			user = response.user,
			accessToken = response.tokens.accessToken,
			refreshToken = response.tokens.refreshToken,
			sessionId = response.tokens.sessionId,
			deviceId = response.tokens.deviceId,
			origin = server.origin,
		)
		storeSession(session)
		connect(session)
		refresh()
	}

	private fun storeSession(session: Session) {
		sessionStore.save(session)
		_state.update { it.copy(session = session) }
		applyPlugins(session)
	}

	private fun connect(session: Session) {
		socket?.close()
		val network = client ?: return
		val server = _state.value.server ?: return
		socket = ChatSocket(server.origin, session, network, ::onFrame, { status ->
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
			is ServerFrame.Ready -> refresh()
			is ServerFrame.Read -> applyReadCursor(frame.conversationId, frame.userId, frame.seq)
			is ServerFrame.Hint -> select(frame.conversationId)
			is ServerFrame.Failure -> _state.update { it.copy(error = frame.message) }
			else -> Unit
		}
	}

	private fun applyReadCursor(conversationId: Long, userId: Long, seq: Long) {
		_state.update { current ->
			val conversations = current.conversations.map { conversation ->
				if (conversation.id != conversationId) {
					conversation
				} else {
					val cursors = conversation.readCursors.toMutableList()
					val index = cursors.indexOfFirst { it.userId == userId }
					if (index >= 0) {
						if (seq > cursors[index].lastReadSeq) {
							cursors[index] = cursors[index].copy(lastReadSeq = seq)
						}
					} else {
						cursors += ReadCursorView(userId, seq)
					}
					val ownRead = if (userId == current.session?.user?.id) {
						maxOf(conversation.lastReadSeq, seq)
					} else {
						conversation.lastReadSeq
					}
					conversation.copy(readCursors = cursors, lastReadSeq = ownRead)
				}
			}
			current.copy(conversations = conversations)
		}
	}

	fun refresh() = viewModelScope.launch {
		val client = api ?: return@launch
		runCatching { withAuth { client.conversations(it) } }
			.onSuccess { response -> _state.update { it.copy(conversations = response.conversations) } }
			.onFailure { fail(it) }
	}

	fun select(conversationId: Long) = viewModelScope.launch {
		val client = api ?: return@launch
		_state.update { it.copy(selectedId = conversationId, messages = emptyList()) }
		runCatching { withAuth { client.messages(it, conversationId) } }
			.onSuccess { response ->
				_state.update { it.copy(messages = response.messages) }
				response.messages.lastOrNull()?.let { last ->
					runCatching { withAuth { client.markRead(it, conversationId, ReadRequest(last.seq)) } }
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
		val client = api ?: return@launch
		runCatching { withAuth { client.createDirect(it, com.chatbox.android.data.DirectRequest(userId)) } }
			.onSuccess { response -> refresh(); select(response.conversation.id) }
			.onFailure { fail(it) }
	}

	fun addContact(userId: String) = viewModelScope.launch {
		val client = api ?: return@launch
		runCatching { withAuth { client.addContact(it, com.chatbox.android.data.DirectRequest(userId)) } }
			.onFailure { fail(it) }
	}

	fun logout() {
		socket?.close()
		socket = null
		sessionStore.clear()
		_state.update {
			it.copy(
				session = null,
				conversations = emptyList(),
				messages = emptyList(),
				selectedId = null,
				status = "offline",
				timeSettings = TimeDisplaySettings(),
				error = null,
				busy = false,
			)
		}
	}

	fun setLocale(locale: String) {
		_state.update { it.copy(locale = locale) }
		_state.value.session?.let { pluginStore.setActiveLocale(it.user.userId, locale) }
	}

	fun setTheme(theme: String) {
		_state.update { it.copy(theme = theme) }
		_state.value.session?.let { pluginStore.setActiveTheme(it.user.userId, theme) }
	}

	fun setTimeSettings(settings: TimeDisplaySettings) {
		val session = _state.value.session ?: return
		val next = normalizeTimeSettings(settings)
		timeStore.save(session.user.id, next)
		_state.update { it.copy(timeSettings = next) }
	}

	fun resetTimeSettings() {
		val session = _state.value.session ?: return
		timeStore.reset(session.user.id)
		_state.update { it.copy(timeSettings = TimeDisplaySettings()) }
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
		_state.update {
			it.copy(
				plugins = plugins,
				locale = locale,
				theme = theme,
				timeSettings = timeStore.load(session.user.id),
			)
		}
	}

	fun dismissError() = _state.update { it.copy(error = null) }

	private fun requireApi(): ChatBoxApi =
		api ?: throw IllegalStateException("choose a server before signing in")

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
			val refreshed = requireApi().refresh(RefreshRequest(session.refreshToken))
			val next = session.copy(
				accessToken = refreshed.tokens.accessToken,
				refreshToken = refreshed.tokens.refreshToken,
				sessionId = refreshed.tokens.sessionId,
				deviceId = refreshed.tokens.deviceId,
			)
			storeSession(next)
			block("Bearer ${next.accessToken}")
		}
	}

	private fun fail(error: Throwable) {
		val causes = generateSequence(error) { it.cause }.toList()
		val required = causes.filterIsInstance<FingerprintRequiredException>().firstOrNull()
		if (required != null) {
			val server = _state.value.server
			if (server != null) {
				_state.update {
					it.copy(fingerprintPrompt = FingerprintPrompt(server, required.info, changed = false))
				}
				return
			}
		}
		val changed = causes.filterIsInstance<FingerprintChangedException>().firstOrNull()
		if (changed != null) {
			val server = _state.value.server
			if (server != null) {
				_state.update {
					it.copy(
						fingerprintPrompt = FingerprintPrompt(
							server,
							changed.info,
							changed = true,
							expected = changed.expected,
						),
					)
				}
				return
			}
		}
		val certificateProblem = causes.any {
			it is CertificateException || it is SSLPeerUnverifiedException
		}
		if (certificateProblem) {
			val info = capture.last()
			if (info != null) {
				_state.update { it.copy(trustPrompt = info, error = null) }
				return
			}
		}
		val message = when {
			error is HttpException -> "request failed (${error.code()})"
			causes.any { it is CleartextBlockedException } -> error.message ?: "cleartext is not allowed"
			else -> error.message ?: "request failed"
		}
		_state.update { it.copy(error = message) }
	}

	private fun InputStream.readCertificateBytes(): ByteArray {
		val output = ByteArrayOutputStream()
		val buffer = ByteArray(8192)
		var total = 0
		while (true) {
			val read = read(buffer)
			if (read < 0) break
			total += read
			require(total <= Certificates.MAX_FILE_BYTES) {
				"the certificate file is too large (max 64 KiB)"
			}
			output.write(buffer, 0, read)
		}
		return output.toByteArray()
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
