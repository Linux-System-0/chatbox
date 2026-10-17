package com.chatbox.android.data

import com.jakewharton.retrofit2.converter.kotlinx.serialization.asConverterFactory
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import retrofit2.Retrofit

/**
 * Builds the Retrofit API for one origin using the shared, policy-enforcing
 * OkHttp client. The client is never constructed here (see ADR 0008).
 */
object ApiClient {
	private val json = Json {
		ignoreUnknownKeys = true
		explicitNulls = false
	}

	fun create(baseUrl: String, client: OkHttpClient): ChatBoxApi {
		return Retrofit.Builder()
			.baseUrl(baseUrl.trimEnd('/') + "/")
			.client(client)
			.addConverterFactory(json.asConverterFactory("application/json".toMediaType()))
			.build()
			.create(ChatBoxApi::class.java)
	}

	fun json(): Json = json
}
