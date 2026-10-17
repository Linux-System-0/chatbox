package com.chatbox.android.network

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class AllowlistEntryTest {

	private fun dns(pattern: String) = AllowlistEntry.parse(AllowlistKind.DNS, pattern)

	private fun matches(pattern: String, host: String) = HostMatcher.matches(dns(pattern), host)

	@Test
	fun matchesOneLabelWithSingleWildcard() {
		assertTrue(matches("*.example.com", "a.example.com"))
		assertFalse(matches("*.example.com", "a.b.example.com"))
		assertFalse(matches("*.example.com", "example.com"))
		assertTrue(matches("*.example.com", "A.Example.COM."))
	}

	@Test
	fun matchesOneOrMoreLabelsWithDoubleWildcard() {
		assertTrue(matches("**.example.com", "a.example.com"))
		assertTrue(matches("**.example.com", "a.b.example.com"))
		assertFalse(matches("**.example.com", "example.com"))
	}

	@Test
	fun supportsWildcardsInsideALabel() {
		assertTrue(matches("chat-*.example.com", "chat-eu.example.com"))
		assertTrue(matches("chat-*.example.com", "chat-eu-eu.example.com"))
		assertFalse(matches("chat-*.example.com", "chat.example.com"))
	}

	@Test
	fun anchorsTheWholeHost() {
		assertFalse(matches("*.example.com", "evil-example.com"))
		assertFalse(matches("example.com", "evil-example.com"))
		assertFalse(matches("example.com", "example.com.evil.test"))
		assertTrue(matches("example.com", "example.com"))
		assertTrue(matches("example.com", "example.com."))
	}

	@Test
	fun requiresALiteralLabel() {
		assertThrows(AddressException::class.java) { dns("*") }
		assertThrows(AddressException::class.java) { dns("**") }
		assertThrows(AddressException::class.java) { dns("*.*") }
		assertThrows(AddressException::class.java) { dns("*.**") }
	}

	@Test
	fun rejectsMalformedPatterns() {
		assertThrows(AddressException::class.java) { dns("") }
		assertThrows(AddressException::class.java) { dns("exa mple.com") }
		assertThrows(AddressException::class.java) { dns("example.com:80") }
		assertThrows(AddressException::class.java) { dns("example.com/path") }
		assertThrows(AddressException::class.java) { dns("example..com") }
		assertThrows(AddressException::class.java) { dns("a**b.example.com") }
		assertThrows(AddressException::class.java) { dns("10.0.0.1") }
	}

	@Test
	fun matchesExactIpLiterals() {
		val entry = AllowlistEntry.parse(AllowlistKind.IP, "10.0.0.5")
		assertEquals(AllowlistKind.IP, entry.kind)
		assertEquals("10.0.0.5", entry.value)
		assertTrue(HostMatcher.matches(entry, "10.0.0.5"))
		assertFalse(HostMatcher.matches(entry, "10.0.0.6"))
		assertFalse(HostMatcher.matches(entry, "example.com"))

		val v6 = AllowlistEntry.parse(AllowlistKind.IP, "[fd00::1]")
		assertEquals("fd00::1", v6.value)
		assertTrue(HostMatcher.matches(v6, "fd00::1"))
		assertTrue(HostMatcher.matches(v6, "[fd00::1]"))
		assertFalse(HostMatcher.matches(v6, "fd00::2"))
	}

	@Test
	fun masksAndMatchesIpv4Cidr() {
		val entry = AllowlistEntry.parse(AllowlistKind.CIDR, "10.0.0.5/8")
		assertEquals(AllowlistKind.CIDR, entry.kind)
		assertEquals("10.0.0.0/8", entry.value)
		assertTrue(HostMatcher.matches(entry, "10.1.2.3"))
		assertTrue(HostMatcher.matches(entry, "10.255.255.255"))
		assertFalse(HostMatcher.matches(entry, "11.0.0.1"))
		assertFalse(HostMatcher.matches(entry, "example.com"))
	}

	@Test
	fun masksAndMatchesIpv6Cidr() {
		val entry = AllowlistEntry.parse(AllowlistKind.CIDR, "fd00::/8")
		assertEquals("fd00::/8", entry.value)
		assertTrue(HostMatcher.matches(entry, "fd12:3456::1"))
		assertFalse(HostMatcher.matches(entry, "fe80::1"))
		assertFalse(HostMatcher.matches(entry, "10.0.0.1"))

		val masked = AllowlistEntry.parse(AllowlistKind.CIDR, "2001:db8:1234:5678::/32")
		assertEquals("2001:db8::/32", masked.value)
		assertTrue(HostMatcher.matches(masked, "2001:db8:ffff::1"))
		assertFalse(HostMatcher.matches(masked, "2001:db9::1"))
	}

	@Test
	fun rejectsInvalidPrefixes() {
		assertThrows(AddressException::class.java) { AllowlistEntry.parse(AllowlistKind.CIDR, "10.0.0.0/33") }
		assertThrows(AddressException::class.java) { AllowlistEntry.parse(AllowlistKind.CIDR, "fd00::/129") }
		assertThrows(AddressException::class.java) { AllowlistEntry.parse(AllowlistKind.CIDR, "fd00::/-1") }
		assertThrows(AddressException::class.java) { AllowlistEntry.parse(AllowlistKind.CIDR, "10.0.0.0/x") }
		assertThrows(AddressException::class.java) { AllowlistEntry.parse(AllowlistKind.CIDR, "10.0.0.0") }
		assertThrows(AddressException::class.java) { AllowlistEntry.parse(AllowlistKind.CIDR, "10.0.0.0/8/8") }
		assertThrows(AddressException::class.java) { AllowlistEntry.parse(AllowlistKind.CIDR, "example.com/8") }
		assertThrows(AddressException::class.java) { AllowlistEntry.parse(AllowlistKind.IP, "10.0.0.0/8") }
	}

	@Test
	fun serializesAndRestoresEntries() {
		val entries = listOf(
			AllowlistEntry.parse(AllowlistKind.DNS, "*.example.com"),
			AllowlistEntry.parse(AllowlistKind.IP, "10.0.0.1"),
			AllowlistEntry.parse(AllowlistKind.CIDR, "fd00::/8"),
		)
		for (entry in entries) {
			val restored = AllowlistEntry.parseSerialized(entry.serialized)
			assertEquals(entry, restored)
		}
		assertEquals(null, AllowlistEntry.parseSerialized("bogus:value"))
	}
}
