package com.chatbox.android.network

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

class ServerAddressTest {

	@Test
	fun parsesHostAndPort() {
		val address = ServerAddress.parse("10.0.2.2:8080")
		assertEquals(ServerAddress.Scheme.HTTP, address.scheme)
		assertEquals("10.0.2.2", address.host)
		assertEquals(8080, address.port)
		assertEquals("http://10.0.2.2:8080", address.origin)
	}

	@Test
	fun defaultsThePortPerScheme() {
		assertEquals("http://example.com", ServerAddress.parse("example.com").origin)
		assertEquals("https://example.com", ServerAddress.parse("example.com", ServerAddress.Scheme.HTTPS).origin)
		assertEquals(80, ServerAddress.parse("example.com").effectivePort)
		assertEquals(443, ServerAddress.parse("example.com", ServerAddress.Scheme.HTTPS).effectivePort)
	}

	@Test
	fun omitsTheDefaultPortFromTheNormalizedForm() {
		assertEquals("http://example.com", ServerAddress.parse("example.com:80").origin)
		assertEquals("https://example.com", ServerAddress.parse("https://example.com:443").origin)
		assertEquals("https://example.com:8443", ServerAddress.parse("https://example.com:8443").origin)
	}

	@Test
	fun pastedSchemeOverridesTheToggle() {
		assertEquals(ServerAddress.Scheme.HTTPS, ServerAddress.parse("https://example.com").scheme)
		assertEquals(ServerAddress.Scheme.HTTP, ServerAddress.parse("http://example.com", ServerAddress.Scheme.HTTPS).scheme)
		assertEquals("https://example.com", ServerAddress.parse("HTTPS://Example.COM:443").origin)
	}

	@Test
	fun lowercasesAndStripsTheTrailingDot() {
		assertEquals("http://example.com", ServerAddress.parse("Example.COM.").origin)
	}

	@Test
	fun convertsInternationalNamesToPunycode() {
		assertEquals("http://xn--mnich-kva.example", ServerAddress.parse("münich.example").origin)
	}

	@Test
	fun canonicalizesIpv6() {
		assertEquals("http://[::1]", ServerAddress.parse("::1").origin)
		assertEquals("http://[::1]:8080", ServerAddress.parse("[::1]:8080").origin)
		assertEquals("http://[2001:db8::1]", ServerAddress.parse("2001:db8:0:0:0:0:0:1").origin)
		assertEquals("https://[fd00::1]", ServerAddress.parse("https://[fd00::1]").origin)
	}

	@Test
	fun buildsNormalizedAddressesFromUrlParts() {
		assertEquals("https://[::1]", ServerAddress.of("https", "::1", 443).origin)
		assertEquals("https://[::1]:8443", ServerAddress.of("https", "[::1]", 8443).origin)
		assertEquals("http://127.0.0.1:8080", ServerAddress.of("http", "127.0.0.1", 8080).origin)
	}

	@Test
	fun rejectsPathsQueriesFragmentsAndUserInfo() {
		assertThrows(AddressException::class.java) { ServerAddress.parse("example.com/path") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("example.com?x=1") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("example.com#fragment") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("user@example.com") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("http://example.com/") }
	}

	@Test
	fun rejectsWhitespaceAndEmptyHosts() {
		assertThrows(AddressException::class.java) { ServerAddress.parse("exa mple.com") }
		assertThrows(AddressException::class.java) { ServerAddress.parse(" example.com") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("http://") }
		assertThrows(AddressException::class.java) { ServerAddress.parse(":8080") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("[]:8080") }
	}

	@Test
	fun rejectsInvalidPorts() {
		assertThrows(AddressException::class.java) { ServerAddress.parse("example.com:0") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("example.com:65536") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("example.com:") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("example.com:abc") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("[::1]:70000") }
	}

	@Test
	fun rejectsInvalidHosts() {
		assertThrows(AddressException::class.java) { ServerAddress.parse("*.example.com") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("010.0.0.1") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("999.0.0.1") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("example..com") }
		assertThrows(AddressException::class.java) { ServerAddress.parse("-example.com") }
	}

	@Test
	fun normalizeHostKeepsLiteralsAndNamesCanonical() {
		assertEquals("example.com", HostMatcher.normalizeHost("Example.COM."))
		assertEquals("10.0.0.5", HostMatcher.normalizeHost("10.0.0.5"))
		assertEquals("::1", HostMatcher.normalizeHost("[::1]"))
		assertEquals("fd00::1", HostMatcher.normalizeHost("fd00:0:0:0:0:0:0:1"))
		assertNull(runCatching { HostMatcher.normalizeHost("bad host") }.getOrNull())
	}
}
