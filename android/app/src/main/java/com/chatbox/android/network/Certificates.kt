package com.chatbox.android.network

import java.io.ByteArrayInputStream
import java.security.MessageDigest
import java.security.cert.CertificateException
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.util.Base64
import java.util.Date

/** Invalid certificate import input. */
class CertificateImportException(message: String) : IllegalArgumentException(message)

/** Display data for one certificate. */
data class CertificateInfo(
	val subject: String,
	val issuer: String,
	val notBefore: Date,
	val notAfter: Date,
	/** Lowercase SHA-256 of the DER encoding, without separators. */
	val sha256: String,
	val certificate: X509Certificate,
) {
	/** Colon-separated uppercase SHA-256, for out-of-band comparison. */
	val displayFingerprint: String
		get() = sha256.chunked(2).joinToString(":") { it.uppercase() }
}

/** Certificate parsing and fingerprint helpers (§8 of the network document). */
object Certificates {
	const val MAX_FILE_BYTES = 64 * 1024
	const val MAX_CERTIFICATES = 10

	/** Parse a PEM or DER file containing one or more X.509 certificates. */
	fun parse(bytes: ByteArray): List<X509Certificate> {
		if (bytes.isEmpty()) throw CertificateImportException("the certificate file is empty")
		if (bytes.size > MAX_FILE_BYTES) {
			throw CertificateImportException("the certificate file is too large (max 64 KiB)")
		}
		val text = bytes.toString(Charsets.UTF_8)
		if (text.contains("PRIVATE KEY")) {
			throw CertificateImportException("the file contains a private key; only certificates may be imported")
		}
		val factory = try {
			CertificateFactory.getInstance("X.509")
		} catch (error: CertificateException) {
			throw CertificateImportException("X.509 certificates are not supported on this device")
		}
		val parsed = try {
			factory.generateCertificates(ByteArrayInputStream(bytes))
		} catch (_: CertificateException) {
			null
		}
		val certificates = when {
			parsed != null && parsed.isNotEmpty() -> parsed
			else -> runCatching { listOf(factory.generateCertificate(ByteArrayInputStream(bytes))) }.getOrNull()
		} ?: throw CertificateImportException("the file is not a PEM or DER X.509 certificate")
		val x509 = certificates.map { certificate ->
			certificate as? X509Certificate
				?: throw CertificateImportException("the file contains a non-X.509 certificate")
		}
		if (x509.size > MAX_CERTIFICATES) {
			throw CertificateImportException("a file may contain at most $MAX_CERTIFICATES certificates")
		}
		return x509
	}

	fun info(certificate: X509Certificate): CertificateInfo = CertificateInfo(
		subject = certificate.subjectX500Principal.name,
		issuer = certificate.issuerX500Principal.name,
		notBefore = certificate.notBefore,
		notAfter = certificate.notAfter,
		sha256 = sha256(certificate),
		certificate = certificate,
	)

	fun sha256(certificate: X509Certificate): String =
		MessageDigest.getInstance("SHA-256")
			.digest(certificate.encoded)
			.joinToString("") { (it.toInt() and 0xFF).toString(16).padStart(2, '0') }

	fun encode(certificate: X509Certificate): String =
		Base64.getEncoder().withoutPadding().encodeToString(certificate.encoded)

	fun decode(encoded: String): X509Certificate {
		val der = try {
			Base64.getDecoder().decode(encoded)
		} catch (_: IllegalArgumentException) {
			throw CertificateImportException("stored certificate is not valid base64")
		}
		return parse(der).firstOrNull()
			?: throw CertificateImportException("stored certificate is empty")
	}
}
