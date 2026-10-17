package com.chatbox.android.network

import okhttp3.tls.HeldCertificate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class CertificatesTest {

	private val certificate = HeldCertificate.Builder()
		.certificateAuthority(1)
		.commonName("ChatBox test CA")
		.build()

	@Test
	fun parsesPemAndDer() {
		val fromPem = Certificates.parse(certificate.certificatePem().toByteArray())
		assertEquals(1, fromPem.size)
		assertEquals(certificate.certificate, fromPem.first())

		val fromDer = Certificates.parse(certificate.certificate.encoded)
		assertEquals(1, fromDer.size)
		assertEquals(certificate.certificate, fromDer.first())
	}

	@Test
	fun parsesMultiplePemCertificates() {
		val second = HeldCertificate.Builder().certificateAuthority(1).commonName("Second CA").build()
		val bundle = certificate.certificatePem() + second.certificatePem()
		val parsed = Certificates.parse(bundle.toByteArray())
		assertEquals(2, parsed.size)
	}

	@Test
	fun rejectsPrivateKeys() {
		val pem = "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n"
		val error = assertThrows(CertificateImportException::class.java) {
			Certificates.parse(pem.toByteArray())
		}
		assertTrue(error.message!!.contains("private key"))
	}

	@Test
	fun rejectsEmptyAndUnknownFiles() {
		assertThrows(CertificateImportException::class.java) { Certificates.parse(ByteArray(0)) }
		assertThrows(CertificateImportException::class.java) {
			Certificates.parse("not a certificate".toByteArray())
		}
	}

	@Test
	fun rejectsFilesOverTheSizeLimit() {
		assertThrows(CertificateImportException::class.java) {
			Certificates.parse(ByteArray(Certificates.MAX_FILE_BYTES + 1))
		}
	}

	@Test
	fun rejectsMoreThanTenCertificates() {
		val one = HeldCertificate.Builder().certificateAuthority(1).commonName("CA").build().certificatePem()
		val bundle = buildString {
			repeat(Certificates.MAX_CERTIFICATES + 1) { append(one) }
		}
		assertThrows(CertificateImportException::class.java) {
			Certificates.parse(bundle.toByteArray())
		}
	}

	@Test
	fun fingerprintsAreStableAndDisplayable() {
		val info = Certificates.info(certificate.certificate)
		assertEquals(64, info.sha256.length)
		assertTrue(info.sha256.all { it in '0'..'9' || it in 'a'..'f' })
		assertEquals(31, info.displayFingerprint.count { it == ':' })
		assertEquals(info.sha256, Certificates.info(certificate.certificate).sha256)
		assertTrue(info.subject.contains("ChatBox test CA"))
		assertTrue(info.notAfter.after(info.notBefore))
	}

	@Test
	fun encodesAndDecodesCertificates() {
		val encoded = Certificates.encode(certificate.certificate)
		assertEquals(certificate.certificate, Certificates.decode(encoded))
	}
}
