package com.chatbox.android.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class User(
	val id: Long,
	@SerialName("user_id") val userId: String,
	@SerialName("display_name") val displayName: String? = null,
)

@Serializable
data class AuthTokens(
	@SerialName("access_token") val accessToken: String,
	@SerialName("refresh_token") val refreshToken: String,
	@SerialName("session_id") val sessionId: String,
	@SerialName("device_id") val deviceId: Long,
)

@Serializable
data class AuthResponse(val user: User, val tokens: AuthTokens)

@Serializable
data class Session(
	val user: User,
	val accessToken: String,
	val refreshToken: String,
	val sessionId: String,
	val deviceId: Long,
	/** The server origin these tokens belong to (see docs/android-network-security.md §13). */
	val origin: String,
)

@Serializable
data class Message(
	val id: Long,
	@SerialName("conversation_id") val conversationId: Long,
	val seq: Long,
	@SerialName("sender_id") val senderId: Long,
	@SerialName("client_msg_id") val clientMsgId: String,
	val kind: String,
	val content: String? = null,
	@SerialName("created_at") val createdAt: String,
	val mentions: List<Long> = emptyList(),
)

@Serializable
data class ConversationView(
	val id: Long,
	val kind: String,
	val title: String,
	@SerialName("last_message") val lastMessage: Message? = null,
	@SerialName("last_read_seq") val lastReadSeq: Long = 0,
	val unread: Long = 0,
)

@Serializable
data class ContactView(val user: User, val remark: String? = null, @SerialName("created_at") val createdAt: String = "")

@Serializable
data class BlockView(val user: User, @SerialName("created_at") val createdAt: String = "")

@Serializable
data class LoginRequest(
	@SerialName("user_id") val userId: String,
	val password: String,
	@SerialName("device_name") val deviceName: String = "android",
	val platform: String = "android",
)

@Serializable
data class RegisterRequest(
	@SerialName("user_id") val userId: String,
	val password: String,
	@SerialName("display_name") val displayName: String? = null,
	@SerialName("device_name") val deviceName: String = "android",
	val platform: String = "android",
)

@Serializable
data class RefreshRequest(@SerialName("refresh_token") val refreshToken: String)

@Serializable
data class DirectRequest(@SerialName("user_id") val userId: String)

@Serializable
data class SendRequest(
	@SerialName("client_msg_id") val clientMsgId: String,
	val kind: String = "text",
	val content: String? = null,
	@SerialName("attachment_id") val attachmentId: Long? = null,
)

@Serializable
data class ConversationsResponse(val conversations: List<ConversationView>)

@Serializable
data class MessagesResponse(val messages: List<Message>)

@Serializable
data class MessageResponse(val message: Message)

@Serializable
data class DirectResponse(val conversation: ConversationView)

@Serializable
data class ContactsResponse(val contacts: List<ContactView>)

@Serializable
data class BlocksResponse(val blocks: List<BlockView>)
