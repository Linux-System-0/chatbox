package com.chatbox.android.network

import java.security.cert.X509Certificate
import java.util.Locale

/** The result of evaluating the network policy for one request hop. */
sealed interface PolicyDecision {
	data object Allow : PolicyDecision
	data object Tls : PolicyDecision
	data class CleartextDenied(val host: String) : PolicyDecision
}

/**
 * The user-controlled trust policy (see docs/android-network-security.md §7).
 *
 * This type is deliberately free of Android dependencies so it can be unit
 * tested and shared by the REST client, the WebSocket, and the health probe.
 */
data class NetworkPolicy(
	val trustSystemCAs: Boolean = true,
	val verificationDisabled: Boolean = false,
	val allowlist: List<AllowlistEntry> = emptyList(),
	val customCertificates: List<X509Certificate> = emptyList(),
) {
	fun decisionFor(scheme: String, host: String): PolicyDecision {
		if (verificationDisabled) return PolicyDecision.Allow
		return when (scheme.lowercase(Locale.ROOT)) {
			"http", "ws" ->
				if (allowlist.any { HostMatcher.matches(it, host) }) {
					PolicyDecision.Allow
				} else {
					PolicyDecision.CleartextDenied(host)
				}

			"https", "wss" -> PolicyDecision.Tls
			else -> PolicyDecision.CleartextDenied(host)
		}
	}

	/** Whether the host is explicitly allowlisted, independent of the verification switch. */
	fun allowsCleartextHost(host: String): Boolean = allowlist.any { HostMatcher.matches(it, host) }
}
