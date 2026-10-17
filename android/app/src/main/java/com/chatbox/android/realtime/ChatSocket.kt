package com.chatbox.android.realtime

import android.os.Handler
import android.os.Looper
import com.chatbox.android.data.Message
import com.chatbox.android.data.Session
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.UUID
import java.util.concurrent.TimeUnit

/** A frame pushed by the server (see docs/realtime-protocol.md). */
sealed interface ServerFrame {
	data class Ready(val serverTime: String) : ServerFrame
	data class Ack(val clientMsgId: String, val conversationId: Long, val seq: Long) : ServerFrame
	data class Incoming(val message: Message) : ServerFrame
	data class Hint(val conversationId: Long, val seq: Long) : ServerFrame
	data class Presence(val userId: Long, val online: Boolean, val devices: Int) : ServerFrame
	data class Failure(val code: String, val message: String) : ServerFrame
}

/**
 * A reconnecting WebSocket client bound to a session. Wire-format models are
 * identical to the other clients, so a message sent from Android is delivered
 * to web and vice versa.
 */
class ChatSocket(
	private val baseUrl: String,
	private val session: Session,
	private val sharedClient: OkHttpClient,
	private val onFrame: (ServerFrame) -> Unit,
	private val onStatus: (String) -> Unit,
) {
	private val client = sharedClient.newBuilder()
		.pingInterval(25, TimeUnit.SECONDS)
		.build()
	private val handler = Handler(Looper.getMainLooper())
	private val json = Json { ignoreUnknownKeys = true }
	private var socket: WebSocket? = null
	private var delayMs = 500L
	@Volatile private var closed = false

	fun connect() {
		closed = false
		onStatus("connecting")
		// OkHttp expects an http(s) URL for the upgrade request; the policy
		// interceptor evaluates it as a cleartext or TLS hop.
		val wsUrl = baseUrl.trimEnd('/') + "/ws"
		val request = Request.Builder().url(wsUrl).build()
		socket = client.newWebSocket(request, object : WebSocketListener() {
			override fun onOpen(webSocket: WebSocket, response: Response) {
				webSocket.send(
					buildJsonObject {
						put("type", "auth")
						put("token", session.accessToken)
						put("device_id", session.deviceId)
					}.toString(),
				)
			}

			override fun onMessage(webSocket: WebSocket, text: String) {
				parse(text)?.let { frame ->
					if (frame is ServerFrame.Ready) {
						delayMs = 500
						onStatus("online")
					}
					onFrame(frame)
				}
			}

			override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
				onStatus("offline")
				scheduleReconnect()
			}

			override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
				onStatus("offline")
				if (!closed) scheduleReconnect()
			}
		})
	}

	private fun scheduleReconnect() {
		if (closed) return
		handler.postDelayed({ if (!closed) connect() }, delayMs)
		delayMs = (delayMs * 2).coerceAtMost(15_000)
	}

	fun send(frame: JsonObject) {
		socket?.send(frame.toString())
	}

	fun sendText(conversationId: Long, content: String) {
		send(
			buildJsonObject {
				put("type", "send")
				put("client_msg_id", UUID.randomUUID().toString())
				put("conversation_id", conversationId)
				put("kind", "text")
				put("content", content)
			},
		)
	}

	fun acknowledge(conversationId: Long, seq: Long) {
		send(
			buildJsonObject {
				put("type", "ack")
				put("conversation_id", conversationId)
				put("seq", seq)
			},
		)
	}

	fun close() {
		closed = true
		socket?.close(1000, null)
	}

	private fun parse(text: String): ServerFrame? = runCatching {
		val root = json.parseToJsonElement(text).jsonObject
		when (root["type"]?.jsonPrimitive?.content) {
			"ready" -> ServerFrame.Ready(root["server_time"]?.jsonPrimitive?.content.orEmpty())
			"ack" -> ServerFrame.Ack(
				root["client_msg_id"]?.jsonPrimitive?.content.orEmpty(),
				root["conversation_id"]?.jsonPrimitive?.long ?: 0L,
				root["seq"]?.jsonPrimitive?.long ?: 0L,
			)
			"message" -> ServerFrame.Incoming(
				json.decodeFromJsonElement(Message.serializer(), root["message"]!!.jsonObject),
			)
			"message_hint" -> ServerFrame.Hint(
				root["conversation_id"]?.jsonPrimitive?.long ?: 0L,
				root["seq"]?.jsonPrimitive?.long ?: 0L,
			)
			"presence" -> ServerFrame.Presence(
				root["user_id"]?.jsonPrimitive?.long ?: 0L,
				root["online"]?.jsonPrimitive?.content == "true",
				root["devices"]?.jsonPrimitive?.content?.toIntOrNull() ?: 0,
			)
			"error" -> ServerFrame.Failure(
				root["code"]?.jsonPrimitive?.content.orEmpty(),
				root["message"]?.jsonPrimitive?.content.orEmpty(),
			)
			else -> null
		}
	}.getOrNull()
}
