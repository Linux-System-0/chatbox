package com.chatbox.android.network

import java.security.KeyStore
import java.security.SecureRandom
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import java.util.concurrent.atomic.AtomicReference
import javax.net.ssl.SSLContext
import javax.net.ssl.TrustManager
import javax.net.ssl.TrustManagerFactory
import javax.net.ssl.X509TrustManager

/** Holds the peer chain presented by the last TLS handshake, even a failed one. */
class CertificateCapture {
	private val chain = AtomicReference<List<X509Certificate>?>(null)

	fun remember(certificates: Array<out X509Certificate>) {
		chain.set(certificates.toList())
	}

	fun last(): CertificateInfo? {
		val leaf = chain.get()?.firstOrNull() ?: return null
		return Certificates.info(leaf)
	}
}

/**
 * Tries the system trust manager and then the user-managed anchors and fails
 * only when both reject the chain (§8).
 */
class PolicyTrustManager(
	private val system: X509TrustManager?,
	private val custom: X509TrustManager?,
	private val capture: CertificateCapture?,
) : X509TrustManager {

	override fun checkClientTrusted(chain: Array<out X509Certificate>, authType: String?) =
		check(chain, authType)

	override fun checkServerTrusted(chain: Array<out X509Certificate>, authType: String?) =
		check(chain, authType)

	private fun check(chain: Array<out X509Certificate>, authType: String?) {
		capture?.remember(chain)
		if (chain.isEmpty()) throw CertificateException("the peer presented no certificate")
		var failure: CertificateException? = null
		if (system != null) {
			try {
				system.checkServerTrusted(chain, authType)
				return
			} catch (error: CertificateException) {
				failure = error
			}
		}
		if (custom != null) {
			try {
				custom.checkServerTrusted(chain, authType)
				return
			} catch (error: CertificateException) {
				failure = error
			}
		}
		throw failure ?: CertificateException("no trust anchors are configured")
	}

	override fun getAcceptedIssuers(): Array<X509Certificate> {
		val issuers = mutableListOf<X509Certificate>()
		system?.acceptedIssuers?.let { issuers.addAll(it) }
		custom?.acceptedIssuers?.let { issuers.addAll(it) }
		return issuers.distinctBy { it.encoded.toList() }.toTypedArray()
	}
}

/** Accepts everything; only installed after the user confirms disabling verification. */
class AcceptAllTrustManager(private val capture: CertificateCapture?) : X509TrustManager {
	override fun checkClientTrusted(chain: Array<out X509Certificate>, authType: String?) {
		capture?.remember(chain)
	}

	override fun checkServerTrusted(chain: Array<out X509Certificate>, authType: String?) {
		capture?.remember(chain)
	}

	override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
}

object TrustAnchors {

	fun systemManager(): X509TrustManager {
		val factory = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm())
		factory.init(null as KeyStore?)
		return factory.trustManagers.filterIsInstance<X509TrustManager>().firstOrNull()
			?: throw IllegalStateException("the device has no X.509 trust manager")
	}

	fun managerFor(certificates: List<X509Certificate>): X509TrustManager? {
		if (certificates.isEmpty()) return null
		val keyStore = KeyStore.getInstance(KeyStore.getDefaultType())
		keyStore.load(null, null)
		certificates.forEachIndexed { index, certificate ->
			keyStore.setCertificateEntry("custom-$index", certificate)
		}
		val factory = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm())
		factory.init(keyStore)
		return factory.trustManagers.filterIsInstance<X509TrustManager>().firstOrNull()
			?: throw IllegalStateException("could not build a trust manager for the custom CAs")
	}

	fun create(policy: NetworkPolicy, capture: CertificateCapture?): X509TrustManager {
		if (policy.verificationDisabled) return AcceptAllTrustManager(capture)
		return PolicyTrustManager(
			system = if (policy.trustSystemCAs) systemManager() else null,
			custom = managerFor(policy.customCertificates),
			capture = capture,
		)
	}

	fun sslContext(trustManager: X509TrustManager): SSLContext =
		SSLContext.getInstance("TLS").apply {
			init(null, arrayOf<TrustManager>(trustManager), SecureRandom())
		}
}
