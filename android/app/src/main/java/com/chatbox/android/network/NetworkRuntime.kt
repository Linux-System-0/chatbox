package com.chatbox.android.network

import java.io.IOException
import java.security.cert.CertificateException
import java.security.cert.CertPathValidatorException
import java.security.cert.X509Certificate
import java.util.concurrent.TimeUnit
import javax.net.ssl.SSLPeerUnverifiedException
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.Interceptor
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response

/** The request was blocked because its cleartext host is not allowlisted. */
class CleartextBlockedException(val host: String) :
	IOException("cleartext traffic to $host is not allowed")

/** This HTTPS origin has no confirmed certificate fingerprint yet. */
class FingerprintRequiredException(val info: CertificateInfo) :
	IOException("confirm the certificate fingerprint for this server")

/** The presented certificate does not match the confirmed fingerprint for the origin. */
class FingerprintChangedException(val expected: String, val info: CertificateInfo) :
	IOException("the certificate fingerprint changed for this server")

/** Read access to confirmed fingerprints, implemented by the policy store. */
fun interface FingerprintStore {
	fun knownFingerprint(origin: String): String?
}

/**
 * Enforces the policy on every network hop: cleartext hosts are matched against
 * the allowlist and, for the configured HTTPS origin, the presented certificate
 * must match the confirmed SHA-256 fingerprint.
 */
class PolicyInterceptor(
	private val policy: NetworkPolicy,
	private val origin: String,
	private val fingerprints: FingerprintStore,
) : Interceptor {

	override fun intercept(chain: Interceptor.Chain): Response {
		val request = chain.request()
		val url = request.url
		when (val decision = policy.decisionFor(url.scheme, url.host)) {
			is PolicyDecision.CleartextDenied -> throw CleartextBlockedException(decision.host)
			PolicyDecision.Allow, PolicyDecision.Tls -> Unit
		}
		val response = chain.proceed(request)
		if (url.scheme == "https" && !policy.verificationDisabled) {
			val requestOrigin = runCatching {
				ServerAddress.of(url.scheme, url.host, url.port).origin
			}.getOrNull()
			if (requestOrigin == origin) {
				val leaf = response.handshake?.peerCertificates?.firstOrNull() as? X509Certificate
				if (leaf != null) {
					val info = Certificates.info(leaf)
					val known = fingerprints.knownFingerprint(origin)
					if (known == null) {
						response.close()
						throw FingerprintRequiredException(info)
					}
					if (!known.equals(info.sha256, ignoreCase = true)) {
						response.close()
						throw FingerprintChangedException(known, info)
					}
				}
			}
		}
		return response
	}
}

/**
 * The single factory for OkHttp clients. Every network path in the app builds
 * its client here, so the runtime policy cannot be bypassed accidentally.
 */
class NetworkClientFactory(
	val policy: NetworkPolicy,
	private val origin: String,
	private val fingerprints: FingerprintStore,
) {
	val capture = CertificateCapture()

	fun create(): OkHttpClient {
		val trustManager = TrustAnchors.create(policy, capture)
		val sslContext = TrustAnchors.sslContext(trustManager)
		val builder = OkHttpClient.Builder()
			.connectTimeout(15, TimeUnit.SECONDS)
			.readTimeout(30, TimeUnit.SECONDS)
			.sslSocketFactory(sslContext.socketFactory, trustManager)
			.addNetworkInterceptor(PolicyInterceptor(policy, origin, fingerprints))
		if (policy.verificationDisabled) {
			builder.hostnameVerifier { _, _ -> true }
		}
		return builder.build()
	}
}

enum class ProbeFailureKind {
	InvalidInput,
	Dns,
	ConnectionRefused,
	Timeout,
	TlsHandshake,
	CertificateTrust,
	FingerprintRequired,
	FingerprintChanged,
	CleartextBlocked,
	HttpStatus,
	Unknown,
}

data class ProbeFailure(
	val kind: ProbeFailureKind,
	val detail: String,
	val certificate: CertificateInfo? = null,
	val expectedFingerprint: String? = null,
)

sealed interface ProbeOutcome {
	data class Success(val origin: String) : ProbeOutcome
	data class Failure(val failure: ProbeFailure) : ProbeOutcome
}

/** Unauthenticated liveness probe sharing the app's policy and client (§12). */
class HealthProbe(
	private val client: OkHttpClient,
	private val capture: CertificateCapture,
	private val timeoutMs: Long = 5_000,
) {

	suspend fun probe(origin: String): ProbeOutcome {
		val url = "$origin/api/v1/health/live".toHttpUrlOrNull()
			?: return ProbeOutcome.Failure(ProbeFailure(ProbeFailureKind.InvalidInput, "invalid server address"))
		val request = Request.Builder().url(url).get().build()
		val call = client.newBuilder()
			.callTimeout(timeoutMs, TimeUnit.MILLISECONDS)
			.build()
			.newCall(request)
		return suspendCancellableCoroutine { continuation ->
			continuation.invokeOnCancellation { call.cancel() }
			call.enqueue(object : Callback {
				override fun onFailure(call: Call, error: java.io.IOException) {
					if (!continuation.isActive) return
					continuation.resume(ProbeOutcome.Failure(classify(error)))
				}

				override fun onResponse(call: Call, response: Response) {
					response.use {
						if (!continuation.isActive) return
						if (it.isSuccessful) {
							continuation.resume(ProbeOutcome.Success(origin))
						} else {
							continuation.resume(
								ProbeOutcome.Failure(
									ProbeFailure(
										ProbeFailureKind.HttpStatus,
										"the server answered with HTTP ${it.code}",
									),
								),
							)
						}
					}
				}
			})
		}
	}

	private fun classify(error: Throwable): ProbeFailure {
		val chain = generateSequence(error) { it.cause }.toList()
		val fingerprintRequired = chain.filterIsInstance<FingerprintRequiredException>().firstOrNull()
		if (fingerprintRequired != null) {
			return ProbeFailure(
				ProbeFailureKind.FingerprintRequired,
				fingerprintRequired.message.orEmpty(),
				fingerprintRequired.info,
			)
		}
		val fingerprintChanged = chain.filterIsInstance<FingerprintChangedException>().firstOrNull()
		if (fingerprintChanged != null) {
			return ProbeFailure(
				ProbeFailureKind.FingerprintChanged,
				fingerprintChanged.message.orEmpty(),
				fingerprintChanged.info,
				fingerprintChanged.expected,
			)
		}
		val cleartextBlocked = chain.filterIsInstance<CleartextBlockedException>().firstOrNull()
		if (cleartextBlocked != null) {
			return ProbeFailure(ProbeFailureKind.CleartextBlocked, cleartextBlocked.message.orEmpty())
		}
		val certificate = capture.last()
		return when {
			chain.any { it is java.net.UnknownHostException } ->
				ProbeFailure(ProbeFailureKind.Dns, "the host name could not be resolved", certificate)

			chain.any {
				it is java.io.InterruptedIOException ||
					it is java.util.concurrent.TimeoutException
			} -> ProbeFailure(ProbeFailureKind.Timeout, "the connection timed out", certificate)

			chain.any { it is java.net.ConnectException } -> ProbeFailure(
				ProbeFailureKind.ConnectionRefused,
				"the connection was refused",
				certificate,
			)

			chain.any { it is SSLPeerUnverifiedException } -> ProbeFailure(
				ProbeFailureKind.CertificateTrust,
				"the certificate does not match the host name",
				certificate,
			)

			chain.any {
				it is CertPathValidatorException ||
					it is java.security.cert.CertPathBuilderException ||
					it is CertificateException ||
					it is java.security.cert.CertStoreException
			} -> ProbeFailure(
				ProbeFailureKind.CertificateTrust,
				"the certificate chain is not trusted",
				certificate,
			)

			chain.any { it is javax.net.ssl.SSLException } ->
				ProbeFailure(ProbeFailureKind.TlsHandshake, "the TLS handshake failed", certificate)

			else -> ProbeFailure(ProbeFailureKind.Unknown, error.message ?: error.javaClass.simpleName, certificate)
		}
	}
}
