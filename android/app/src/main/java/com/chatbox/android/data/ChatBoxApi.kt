package com.chatbox.android.data

import kotlinx.serialization.Serializable
import retrofit2.Response
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.Header
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Query

@Serializable
data class AuthRefreshResponse(val tokens: AuthTokens)

/** The REST control-plane contract (see docs/api.md). */
interface ChatBoxApi {
	@POST("api/v1/auth/login")
	suspend fun login(@Body body: LoginRequest): AuthResponse

	@POST("api/v1/auth/register")
	suspend fun register(@Body body: RegisterRequest): AuthResponse

	@POST("api/v1/auth/refresh")
	suspend fun refresh(@Body body: RefreshRequest): AuthRefreshResponse

	@GET("api/v1/conversations")
	suspend fun conversations(@Header("Authorization") bearer: String): ConversationsResponse

	@POST("api/v1/conversations/direct")
	suspend fun createDirect(
		@Header("Authorization") bearer: String,
		@Body body: DirectRequest,
	): DirectResponse

	@GET("api/v1/conversations/{id}/messages")
	suspend fun messages(
		@Header("Authorization") bearer: String,
		@Path("id") conversationId: Long,
		@Query("limit") limit: Int = 100,
	): MessagesResponse

	@POST("api/v1/conversations/{id}/messages")
	suspend fun send(
		@Header("Authorization") bearer: String,
		@Path("id") conversationId: Long,
		@Body body: SendRequest,
	): MessageResponse

	@POST("api/v1/conversations/{id}/read")
	suspend fun markRead(
		@Header("Authorization") bearer: String,
		@Path("id") conversationId: Long,
		@Body body: ReadRequest,
	): Response<Unit>

	@GET("api/v1/contacts")
	suspend fun contacts(@Header("Authorization") bearer: String): ContactsResponse

	@GET("api/v1/blocks")
	suspend fun blocks(@Header("Authorization") bearer: String): BlocksResponse

	@POST("api/v1/contacts/requests")
	suspend fun addContact(@Header("Authorization") bearer: String, @Body body: DirectRequest)

	@POST("api/v1/blocks/{user_id}")
	suspend fun block(@Header("Authorization") bearer: String, @Path("user_id") userId: String)
}

@Serializable
data class ReadRequest(val seq: Long)
