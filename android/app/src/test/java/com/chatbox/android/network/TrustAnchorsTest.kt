package com.chatbox.android.network

import java.security.cert.CertificateException
import java.util.concurrent.TimeUnit
import okhttp3.tls.HeldCertificate
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class TrustAnchorsTest {

	private val ca = HeldCertificate.Builder()
		.certificateAuthority(1)
		.commonName("ChatBox test CA")
		.build()

	private val leaf = HeldCertificate.Builder()
		.signedBy(ca)
		.commonName("localhost")
		.addSubjectAlternativeName("localhost")
		.build()

	private val chain = arrayOf(leaf.certificate, ca.certificate)

	private fun policy(
		trustSystem: Boolean = false,
		custom: List<java.security.cert.X509Certificate> = listOf(ca.certificate),
		disabled: Boolean = false,
	) = NetworkPolicy(
		trustSystemCAs = trustSystem,
		verificationDisabled = disabled,
		customCertificates = custom,
	)

	@Test
	fun customAnchorsAcceptTheirOwnChain() {
		TrustAnchors.create(policy(), null).checkServerTrusted(chain, "ECDHE_ECDSA")
	}

	@Test
	fun selfSignedServerCertificateCanBeAnAnchor() {
		val selfSigned = HeldCertificate.Builder()
			.commonName("localhost")
			.addSubjectAlternativeName("localhost")
			.build()
		val manager = TrustAnchors.create(policy(custom = listOf(selfSigned.certificate)), null)
		manager.checkServerTrusted(arrayOf(selfSigned.certificate), "ECDHE_ECDSA")
	}

	@Test
	fun systemOnlyRejectsAPrivateCa() {
		val manager = TrustAnchors.create(policy(custom = emptyList()), null)
		assertThrows(CertificateException::class.java) { manager.checkServerTrusted(chain, "ECDHE_ECDSA") }
	}

	@Test
	fun customOnlyRejectsAnUnrelatedCa() {
		val otherCa = HeldCertificate.Builder()
			.certificateAuthority(1)
			.commonName("Unrelated CA")
			.build()
		val manager = TrustAnchors.create(policy(custom = listOf(otherCa.certificate)), null)
		assertThrows(CertificateException::class.java) { manager.checkServerTrusted(chain, "ECDHE_ECDSA") }
	}

	@Test
	fun compositionAcceptsWhenEitherAnchorSetAccepts() {
		val manager = TrustAnchors.create(
			policy(trustSystem = true, custom = listOf(ca.certificate)),
			null,
		)
		manager.checkServerTrusted(chain, "ECDHE_ECDSA")
	}

	@Test
	fun compositionFailsWhenBothAnchorSetsReject() {
		val otherCa = HeldCertificate.Builder().certificateAuthority(1).commonName("Other CA").build()
		val manager = TrustAnchors.create(
			policy(trustSystem = true, custom = listOf(otherCa.certificate)),
			null,
		)
		assertThrows(CertificateException::class.java) { manager.checkServerTrusted(chain, "ECDHE_ECDSA") }
	}

	@Test
	fun rejectsAnEmptyChain() {
		val manager = TrustAnchors.create(policy(), null)
		assertThrows(CertificateException::class.java) {
			manager.checkServerTrusted(emptyArray(), "ECDHE_ECDSA")
		}
	}

	@Test
	fun rejectsAnExpiredCertificate() {
		val now = System.currentTimeMillis()
		val expired = HeldCertificate.Builder()
			.signedBy(ca)
			.commonName("localhost")
			.validityInterval(now - TimeUnit.DAYS.toMillis(2), now - TimeUnit.DAYS.toMillis(1))
			.build()
		val manager = TrustAnchors.create(policy(), null)
		assertThrows(CertificateException::class.java) {
			manager.checkServerTrusted(arrayOf(expired.certificate, ca.certificate), "ECDHE_ECDSA")
		}
	}

	@Test
	fun disablingVerificationAcceptsAnything() {
		val manager = TrustAnchors.create(policy(custom = emptyList(), disabled = true), null)
		manager.checkServerTrusted(chain, "ECDHE_ECDSA")
		manager.checkServerTrusted(emptyArray(), "ECDHE_ECDSA")
		assertTrue(manager is AcceptAllTrustManager)
	}

	@Test
	fun captureKeepsThePresentedChainEvenOnFailure() {
		val capture = CertificateCapture()
		val manager = TrustAnchors.create(policy(custom = emptyList()), capture)
		runCatching { manager.checkServerTrusted(chain, "ECDHE_ECDSA") }
		val info = capture.last()
		assertTrue(info != null)
		assertTrue(info!!.sha256.isNotEmpty())
	}
}
