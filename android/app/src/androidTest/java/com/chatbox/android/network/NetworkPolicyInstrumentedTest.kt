package com.chatbox.android.network

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.IOException
import java.security.cert.CertificateException
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.SocketPolicy
import okhttp3.tls.HandshakeCertificates
import okhttp3.tls.HeldCertificate
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Policy tests over real sockets with MockWebServer and HandshakeCertificates
 * (docs/android-network-security.md §18).
 */
@RunWith(AndroidJUnit4::class)
class NetworkPolicyInstrumentedTest {

	private lateinit var context: Context
	private lateinit var store: NetworkPolicyStore
	private val servers = mutableListOf<MockWebServer>()

	@Before
	fun setUp() {
		context = ApplicationProvider.getApplicationContext()
		context.getSharedPreferences("chatbox_net", Context.MODE_PRIVATE).edit().clear().commit()
		store = NetworkPolicyStore(context)
	}

	@After
	fun tearDown() {
		servers.forEach { runCatching { it.shutdown() } }
		servers.clear()
		context.getSharedPreferences("chatbox_net", Context.MODE_PRIVATE).edit().clear().commit()
	}

	// ------------------------------------------------------------------
	// Cleartext
	// ------------------------------------------------------------------

	@Test
	fun cleartextIsDeniedByDefault() {
		val server = startHttp()
		server.enqueue(MockResponse().setBody("{}"))
		val client = clientFor(origin(server))
		val error = assertThrows(IOException::class.java) { get(client, server.url("/").toString()) }
		assertTrue(error is CleartextBlockedException)
		assertEquals(0, server.requestCount)
	}

	@Test
	fun cleartextIsAllowedForAnAllowlistedHost() {
		val server = startHttp()
		server.enqueue(MockResponse().setBody("ok"))
		store.addAllowlistEntry(kindFor(server), server.hostName)
		val client = clientFor(origin(server))
		get(client, server.url("/").toString()).use { response ->
			assertTrue(response.isSuccessful)
			assertEquals("ok", response.body?.string())
		}
		assertNotNull(server.takeRequest())
	}

	@Test
	fun cleartextIsAllowedForAnAllowlistedCidr() {
		val server = startHttp()
		val host = server.hostName
		if (HostMatcher.isLiteralHost(host)) {
			store.addAllowlistEntry(AllowlistKind.CIDR, "$host/32")
			server.enqueue(MockResponse().setBody("ok"))
			val client = clientFor(origin(server))
			get(client, server.url("/").toString()).use { assertTrue(it.isSuccessful) }
		}
	}

	// ------------------------------------------------------------------
	// TLS and fingerprints
	// ------------------------------------------------------------------

	@Test
	fun customCaIsTrustedAndFirstConnectionRequiresFingerprintConfirmation() = runBlocking {
		val certificate = selfSigned("localhost")
		val server = startHttps(certificate)
		server.enqueue(MockResponse().setBody("{\"status\":\"ok\"}"))
		store.setTrustSystemCAs(false)
		store.addCustomCertificate(certificate.certificate)
		val origin = origin(server)
		val client = clientFor(origin)

		val first = probeFor(origin).probe(origin)
		assertTrue(first is ProbeOutcome.Failure)
		val failure = (first as ProbeOutcome.Failure).failure
		assertEquals(ProbeFailureKind.FingerprintRequired, failure.kind)
		assertEquals(Certificates.sha256(certificate.certificate), failure.certificate?.sha256)

		store.confirmFingerprint(origin, failure.certificate!!.sha256)
		server.enqueue(MockResponse().setBody("{\"status\":\"ok\"}"))
		val second = probeFor(origin).probe(origin)
		assertTrue(second is ProbeOutcome.Success)
	}

	@Test
	fun certificateChangeRequiresANewConfirmation() = runBlocking {
		val firstCertificate = selfSigned("localhost")
		val secondCertificate = selfSigned("localhost")
		val firstServer = startHttps(firstCertificate)
		val port = firstServer.port
		store.setTrustSystemCAs(false)
		store.addCustomCertificate(firstCertificate.certificate)
		store.addCustomCertificate(secondCertificate.certificate)
		val origin = origin(firstServer)
		val client = clientFor(origin)
		firstServer.enqueue(MockResponse().setBody("{}"))
		store.confirmFingerprint(origin, Certificates.sha256(firstCertificate.certificate))
		assertTrue(probeFor(origin).probe(origin) is ProbeOutcome.Success)
		firstServer.shutdown()

		val secondServer = startHttps(secondCertificate, port)
		secondServer.enqueue(MockResponse().setBody("{}"))
		val outcome = probeFor(origin).probe(origin)
		assertTrue(outcome is ProbeOutcome.Failure)
		val failure = (outcome as ProbeOutcome.Failure).failure
		assertEquals(ProbeFailureKind.FingerprintChanged, failure.kind)
		assertEquals(Certificates.sha256(firstCertificate.certificate), failure.expectedFingerprint)
		assertEquals(Certificates.sha256(secondCertificate.certificate), failure.certificate?.sha256)

		store.confirmFingerprint(origin, Certificates.sha256(secondCertificate.certificate))
		secondServer.enqueue(MockResponse().setBody("{}"))
		assertTrue(probeFor(origin).probe(origin) is ProbeOutcome.Success)
	}

	@Test
	fun wrongHostNameIsRejected() = runBlocking {
		val certificate = selfSigned("example.com")
		val server = startHttps(certificate)
		server.enqueue(MockResponse().setBody("{}"))
		store.setTrustSystemCAs(false)
		store.addCustomCertificate(certificate.certificate)
		val origin = origin(server)
		store.confirmFingerprint(origin, Certificates.sha256(certificate.certificate))
		val outcome = probeFor(origin).probe(origin)
		assertTrue(outcome is ProbeOutcome.Failure)
		assertEquals(ProbeFailureKind.CertificateTrust, (outcome as ProbeOutcome.Failure).failure.kind)
	}

	@Test
	fun untrustedCertificateIsClassifiedAsATrustFailure() = runBlocking {
		val certificate = selfSigned("localhost")
		val server = startHttps(certificate)
		server.enqueue(MockResponse().setBody("{}"))
		store.setTrustSystemCAs(false)
		val origin = origin(server)
		val outcome = probeFor(origin).probe(origin)
		assertTrue(outcome is ProbeOutcome.Failure)
		val failure = (outcome as ProbeOutcome.Failure).failure
		assertEquals(ProbeFailureKind.CertificateTrust, failure.kind)
		assertEquals(Certificates.sha256(certificate.certificate), failure.certificate?.sha256)
	}

	// ------------------------------------------------------------------
	// Redirects
	// ------------------------------------------------------------------

	@Test
	fun httpsToHttpRedirectIsBlockedUnlessTheTargetIsAllowlisted() {
		val certificate = selfSigned("localhost")
		val httpsServer = startHttps(certificate)
		val httpServer = startHttp()
		store.setTrustSystemCAs(false)
		store.addCustomCertificate(certificate.certificate)
		val secureOrigin = origin(httpsServer)
		store.confirmFingerprint(secureOrigin, Certificates.sha256(certificate.certificate))
		val client = clientFor(secureOrigin)

		httpsServer.enqueue(
			MockResponse().setResponseCode(302).setHeader("Location", httpServer.url("/target")),
		)
		val error = assertThrows(IOException::class.java) {
			get(client, httpsServer.url("/").toString())
		}
		assertTrue(error is CleartextBlockedException)
		assertEquals(0, httpServer.requestCount)

		store.addAllowlistEntry(kindFor(httpServer), httpServer.hostName)
		val allowedClient = clientFor(secureOrigin)
		httpsServer.enqueue(
			MockResponse().setResponseCode(302).setHeader("Location", httpServer.url("/target")),
		)
		httpServer.enqueue(MockResponse().setBody("ok"))
		get(allowedClient, httpsServer.url("/").toString()).use { response ->
			assertTrue(response.isSuccessful)
			assertEquals("ok", response.body?.string())
		}
		assertEquals(1, httpServer.requestCount)
	}

	@Test
	fun authorizationIsNotForwardedAcrossOrigins() {
		val first = startHttp()
		val second = startHttp()
		store.addAllowlistEntry(kindFor(first), first.hostName)
		store.addAllowlistEntry(kindFor(second), second.hostName)
		val client = clientFor(origin(first))
		first.enqueue(
			MockResponse().setResponseCode(302).setHeader("Location", second.url("/target")),
		)
		second.enqueue(MockResponse().setBody("ok"))
		val request = Request.Builder()
			.url(first.url("/"))
			.header("Authorization", "Bearer secret-token")
			.build()
		client.newCall(request).execute().use { response ->
			assertTrue(response.isSuccessful)
		}
		val forwarded = second.takeRequest()
		assertEquals("/target", forwarded.path)
		assertNull(forwarded.getHeader("Authorization"))
	}

	// ------------------------------------------------------------------
	// Verification switch and probe
	// ------------------------------------------------------------------

	@Test
	fun disablingVerificationAcceptsAnyCertificateAndCleartextHost() = runBlocking {
		val certificate = selfSigned("example.com")
		val httpsServer = startHttps(certificate)
		val httpServer = startHttp()
		store.setVerificationDisabled(true)
		httpsServer.enqueue(MockResponse().setBody("{}"))
		val httpsOutcome = probeFor(origin(httpsServer))
			.probe(origin(httpsServer))
		assertTrue(httpsOutcome is ProbeOutcome.Success)

		httpServer.enqueue(MockResponse().setBody("{}"))
		val httpOutcome = probeFor(origin(httpServer))
			.probe(origin(httpServer))
		assertTrue(httpOutcome is ProbeOutcome.Success)
	}

	@Test
	fun refusedConnectionIsClassifiedAsRefused() = runBlocking {
		val server = startHttp()
		val origin = origin(server)
		store.addAllowlistEntry(kindFor(server), server.hostName)
		server.shutdown()
		val outcome = probeFor(origin).probe(origin)
		assertTrue(outcome is ProbeOutcome.Failure)
		assertEquals(
			ProbeFailureKind.ConnectionRefused,
			(outcome as ProbeOutcome.Failure).failure.kind,
		)
	}

	@Test
	fun hangingServerIsClassifiedAsTimeout() = runBlocking {
		val server = startHttp()
		val origin = origin(server)
		store.addAllowlistEntry(kindFor(server), server.hostName)
		server.enqueue(MockResponse().setSocketPolicy(SocketPolicy.NO_RESPONSE))
		val factory = NetworkClientFactory(store.policy(), origin, store)
		val probe = HealthProbe(factory.create(), factory.capture, timeoutMs = 750)
		val outcome = probe.probe(origin)
		assertTrue(outcome is ProbeOutcome.Failure)
		assertEquals(
			ProbeFailureKind.Timeout,
			(outcome as ProbeOutcome.Failure).failure.kind,
		)
	}

	@Test
	fun probeUsesTheActivePolicy() = runBlocking {
		val server = startHttp()
		val origin = origin(server)
		server.enqueue(MockResponse().setBody("{\"status\":\"ok\"}"))
		val blocked = probeFor(origin).probe(origin)
		assertTrue(blocked is ProbeOutcome.Failure)
		assertEquals(ProbeFailureKind.CleartextBlocked, (blocked as ProbeOutcome.Failure).failure.kind)
		assertEquals(0, server.requestCount)

		store.addAllowlistEntry(kindFor(server), server.hostName)
		server.enqueue(MockResponse().setBody("{\"status\":\"ok\"}"))
		val allowed = probeFor(origin).probe(origin)
		assertTrue(allowed is ProbeOutcome.Success)
		assertEquals("/api/v1/health/live", server.takeRequest().path)
	}

	// ------------------------------------------------------------------
	// Helpers
	// ------------------------------------------------------------------

	private fun clientFor(origin: String): OkHttpClient =
		NetworkClientFactory(store.policy(), origin, store).create()

	private fun probeFor(origin: String): HealthProbe {
		val factory = NetworkClientFactory(store.policy(), origin, store)
		return HealthProbe(factory.create(), factory.capture)
	}

	private fun origin(server: MockWebServer): String =
		ServerAddress.of(server.url("/").scheme, server.url("/").host, server.url("/").port).origin

	private fun kindFor(server: MockWebServer): AllowlistKind =
		if (HostMatcher.isLiteralHost(server.hostName)) AllowlistKind.IP else AllowlistKind.DNS

	private fun startHttp(): MockWebServer {
		val server = MockWebServer()
		server.start()
		servers += server
		return server
	}

	private fun startHttps(certificate: HeldCertificate, port: Int = 0): MockWebServer {
		val server = MockWebServer()
		val handshake = HandshakeCertificates.Builder()
			.heldCertificate(certificate)
			.build()
		server.useHttps(handshake.sslSocketFactory(), false)
		if (port == 0) server.start() else server.start(port)
		servers += server
		return server
	}

	private fun selfSigned(host: String): HeldCertificate =
		HeldCertificate.Builder()
			.commonName(host)
			.addSubjectAlternativeName(host)
			.build()

	private fun get(client: OkHttpClient, url: String) =
		client.newCall(Request.Builder().url(url).build()).execute()
}
