package com.chatbox.android.data

import com.jakewharton.retrofit2.converter.kotlinx.serialization.asConverterFactory
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import retrofit2.Retrofit
import java.util.concurrent.TimeUnit

/** Builds the Retrofit-backed API client. */
object ApiClient {
	private val json = Json {
		ignoreUnknownKeys = true
		explicitNulls = false
	}

	fun create(baseUrl: String): ChatBoxApi {
		val client = OkHttpClient.Builder()
			.connectTimeout(15, TimeUnit.SECONDS)
			.readTimeout(30, TimeUnit.SECONDS)
			.build()
		return Retrofit.Builder()
			.baseUrl(baseUrl.trimEnd('/') + "/")
			.client(client)
			.addConverterFactory(json.asConverterFactory("application/json".toMediaType()))
			.build()
			.create(ChatBoxApi::class.java)
	}

	fun json(): Json = json
}
