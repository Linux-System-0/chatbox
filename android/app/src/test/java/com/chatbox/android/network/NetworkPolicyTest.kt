package com.chatbox.android.network

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NetworkPolicyTest {

	private fun assertAllow(decision: PolicyDecision) = assertTrue(decision is PolicyDecision.Allow)
	private fun assertTls(decision: PolicyDecision) = assertTrue(decision is PolicyDecision.Tls)
	private fun assertDenied(decision: PolicyDecision) =
		assertTrue(decision is PolicyDecision.CleartextDenied)

	@Test
	fun defaultsDenyCleartextAndRequireTls() {
		val policy = NetworkPolicy()
		assertTrue(policy.trustSystemCAs)
		assertFalse(policy.verificationDisabled)
		assertDenied(policy.decisionFor("http", "example.com"))
		assertDenied(policy.decisionFor("ws", "example.com"))
		assertTls(policy.decisionFor("https", "example.com"))
		assertTls(policy.decisionFor("wss", "example.com"))
	}

	@Test
	fun allowlistPermitsMatchingCleartextHosts() {
		val policy = NetworkPolicy(
			allowlist = listOf(AllowlistEntry.parse(AllowlistKind.DNS, "*.lan.example")),
		)
		assertAllow(policy.decisionFor("http", "chat.lan.example"))
		assertDenied(policy.decisionFor("http", "other.example"))
	}

	@Test
	fun disabledVerificationShortCircuitsEverything() {
		val policy = NetworkPolicy(verificationDisabled = true)
		assertAllow(policy.decisionFor("http", "anything.example"))
		assertAllow(policy.decisionFor("https", "anything.example"))
		assertAllow(policy.decisionFor("ws", "127.0.0.1"))
		assertEquals(NetworkPolicy(), NetworkPolicy().copy(verificationDisabled = false))
	}

	@Test
	fun unknownSchemesAreDenied() {
		val policy = NetworkPolicy()
		assertDenied(policy.decisionFor("ftp", "example.com"))
	}

	@Test
	fun allowlistCheckIsIndependentOfTheVerificationSwitch() {
		val policy = NetworkPolicy(
			verificationDisabled = true,
			allowlist = listOf(AllowlistEntry.parse(AllowlistKind.IP, "10.0.0.1")),
		)
		assertTrue(policy.allowsCleartextHost("10.0.0.1"))
		assertFalse(policy.allowsCleartextHost("10.0.0.2"))
	}
}
