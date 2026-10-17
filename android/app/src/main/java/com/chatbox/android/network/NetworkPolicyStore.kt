package com.chatbox.android.network

import android.content.Context
import java.security.cert.X509Certificate
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * Device-level network policy storage (§14).
 *
 * The policy survives logout; the session does not. Only public certificates
 * are stored, never private keys.
 */
class NetworkPolicyStore(context: Context) : FingerprintStore {
	private val prefs = context.getSharedPreferences("chatbox_net", Context.MODE_PRIVATE)
	private val json = Json { ignoreUnknownKeys = true }

	fun policy(): NetworkPolicy = NetworkPolicy(
		trustSystemCAs = prefs.getBoolean(KEY_TRUST_SYSTEM, true),
		verificationDisabled = prefs.getBoolean(KEY_VERIFICATION_DISABLED, false),
		allowlist = allowlistEntries(),
		customCertificates = customCertificates(),
	)

	fun allowlistSerialized(): List<String> = readList(KEY_ALLOWLIST)

	fun allowlistEntries(): List<AllowlistEntry> =
		allowlistSerialized().mapNotNull { AllowlistEntry.parseSerialized(it) }

	fun addAllowlistEntry(kind: AllowlistKind, input: String): AllowlistEntry {
		val entry = AllowlistEntry.parse(kind, input)
		val current = allowlistSerialized()
		if (entry.serialized !in current) {
			writeList(KEY_ALLOWLIST, current + entry.serialized)
		}
		return entry
	}

	fun removeAllowlistEntry(serialized: String) {
		writeList(KEY_ALLOWLIST, allowlistSerialized().filterNot { it == serialized })
	}

	fun customCertificates(): List<X509Certificate> =
		readList(KEY_CUSTOM_CAS).mapNotNull { encoded ->
			runCatching { Certificates.decode(encoded) }.getOrNull()
		}

	/** Import one certificate; returns false when it is already trusted. */
	fun addCustomCertificate(certificate: X509Certificate): Boolean {
		val fingerprint = Certificates.sha256(certificate)
		val current = readList(KEY_CUSTOM_CAS)
		val exists = customCertificates().any { Certificates.sha256(it) == fingerprint }
		if (exists) return false
		writeList(KEY_CUSTOM_CAS, current + Certificates.encode(certificate))
		return true
	}

	fun removeCustomCertificate(sha256: String) {
		writeList(
			KEY_CUSTOM_CAS,
			readList(KEY_CUSTOM_CAS).filterNot { encoded ->
				runCatching { Certificates.sha256(Certificates.decode(encoded)) == sha256 }.getOrDefault(false)
			},
		)
	}

	fun setTrustSystemCAs(enabled: Boolean) {
		prefs.edit().putBoolean(KEY_TRUST_SYSTEM, enabled).apply()
	}

	fun setVerificationDisabled(disabled: Boolean) {
		val editor = prefs.edit().putBoolean(KEY_VERIFICATION_DISABLED, disabled)
		if (disabled) {
			editor.putLong(KEY_DISABLED_AT, System.currentTimeMillis())
		} else {
			editor.remove(KEY_DISABLED_AT)
		}
		editor.apply()
	}

	fun verificationDisabledAt(): Long? {
		if (!prefs.getBoolean(KEY_VERIFICATION_DISABLED, false)) return null
		return prefs.getLong(KEY_DISABLED_AT, 0L).takeIf { it > 0L }
	}

	override fun knownFingerprint(origin: String): String? = fingerprints()[origin]

	/** Store the confirmed SHA-256 fingerprint for an origin. */
	fun confirmFingerprint(origin: String, sha256: String) {
		val next = fingerprints().toMutableMap()
		next[origin] = sha256.lowercase()
		prefs.edit().putString(KEY_FINGERPRINTS, json.encodeToString(next)).apply()
	}

	fun forgetFingerprint(origin: String) {
		val next = fingerprints().toMutableMap()
		next.remove(origin)
		prefs.edit().putString(KEY_FINGERPRINTS, json.encodeToString(next)).apply()
	}

	private fun fingerprints(): Map<String, String> {
		val raw = prefs.getString(KEY_FINGERPRINTS, null) ?: return emptyMap()
		return runCatching { json.decodeFromString<Map<String, String>>(raw) }.getOrDefault(emptyMap())
	}

	private fun readList(key: String): List<String> {
		val raw = prefs.getString(key, null) ?: return emptyList()
		return runCatching { json.decodeFromString<List<String>>(raw) }.getOrDefault(emptyList())
	}

	private fun writeList(key: String, value: List<String>) {
		prefs.edit().putString(key, json.encodeToString(value)).apply()
	}

	private companion object {
		const val KEY_TRUST_SYSTEM = "trust_system_cas"
		const val KEY_VERIFICATION_DISABLED = "verification_disabled"
		const val KEY_DISABLED_AT = "verification_disabled_at"
		const val KEY_ALLOWLIST = "allowlist"
		const val KEY_CUSTOM_CAS = "custom_cas"
		const val KEY_FINGERPRINTS = "fingerprints"
	}
}
