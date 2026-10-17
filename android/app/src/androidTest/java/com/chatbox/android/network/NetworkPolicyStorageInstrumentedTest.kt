package com.chatbox.android.network

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import okhttp3.tls.HeldCertificate
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Policy storage tests (docs/android-network-security.md §14): the policy lives
 * in `chatbox_net`, survives store re-creation, and is scoped per origin for
 * confirmed fingerprints.
 */
@RunWith(AndroidJUnit4::class)
class NetworkPolicyStorageInstrumentedTest {

	private lateinit var context: Context

	@Before
	fun setUp() {
		context = ApplicationProvider.getApplicationContext()
		context.getSharedPreferences("chatbox_net", Context.MODE_PRIVATE).edit().clear().commit()
	}

	@After
	fun tearDown() {
		context.getSharedPreferences("chatbox_net", Context.MODE_PRIVATE).edit().clear().commit()
	}

	@Test
	fun defaultsAreSecure() {
		val store = NetworkPolicyStore(context)
		val policy = store.policy()
		assertTrue(policy.trustSystemCAs)
		assertFalse(policy.verificationDisabled)
		assertTrue(policy.allowlist.isEmpty())
		assertTrue(policy.customCertificates.isEmpty())
		assertNull(store.verificationDisabledAt())
	}

	@Test
	fun policyPersistsAcrossStoreInstances() {
		val certificate = HeldCertificate.Builder()
			.certificateAuthority(1)
			.commonName("ChatBox test CA")
			.build()
		val store = NetworkPolicyStore(context)
		store.setTrustSystemCAs(false)
		store.addAllowlistEntry(AllowlistKind.DNS, "*.lan.example")
		store.addAllowlistEntry(AllowlistKind.CIDR, "10.0.0.5/8")
		store.addCustomCertificate(certificate.certificate)
		store.setVerificationDisabled(true)
		val disabledAt = store.verificationDisabledAt()
		store.confirmFingerprint("https://example.com", Certificates.sha256(certificate.certificate))

		val reloaded = NetworkPolicyStore(context)
		val policy = reloaded.policy()
		assertFalse(policy.trustSystemCAs)
		assertTrue(policy.verificationDisabled)
		assertEquals(2, policy.allowlist.size)
		assertEquals(1, policy.customCertificates.size)
		assertEquals(disabledAt, reloaded.verificationDisabledAt())
		assertEquals(
			Certificates.sha256(certificate.certificate),
			reloaded.knownFingerprint("https://example.com"),
		)
		assertNull(reloaded.knownFingerprint("https://other.example"))

		reloaded.setVerificationDisabled(false)
		assertNull(reloaded.verificationDisabledAt())
	}

	@Test
	fun allowlistEntriesCanBeRemoved() {
		val store = NetworkPolicyStore(context)
		val entry = store.addAllowlistEntry(AllowlistKind.CIDR, "fd00::1/8")
		assertEquals("fd00::/8", entry.value)
		assertEquals(1, store.policy().allowlist.size)
		store.removeAllowlistEntry(entry.serialized)
		assertTrue(store.policy().allowlist.isEmpty())
	}
}
