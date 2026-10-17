package com.chatbox.android.network

import java.net.IDN
import java.util.Locale

/** How the user entered an allowlist entry. */
enum class AllowlistKind(val id: String) {
	DNS("dns"),
	IP("ip"),
	CIDR("cidr"),
	;

	companion object {
		fun fromId(id: String): AllowlistKind? = entries.firstOrNull { it.id == id }
	}
}

/** One cleartext allowlist entry: a DNS pattern, an IP literal, or a CIDR block. */
sealed class AllowlistEntry {
	abstract val kind: AllowlistKind
	abstract val value: String

	val serialized: String get() = "${kind.id}:$value"

	class Dns(override val value: String, private val regex: Regex) : AllowlistEntry() {
		override val kind: AllowlistKind get() = AllowlistKind.DNS

		fun matches(host: String): Boolean = regex.matches(host)

		override fun equals(other: Any?): Boolean = other is Dns && other.value == value

		override fun hashCode(): Int = value.hashCode()

		override fun toString(): String = value
	}

	class Ip(override val value: String, val bytes: ByteArray, val prefix: Int) : AllowlistEntry() {
		override val kind: AllowlistKind get() = if (bytes.size == 4 && prefix == 32 || bytes.size == 16 && prefix == 128) {
			AllowlistKind.IP
		} else {
			AllowlistKind.CIDR
		}

		fun contains(host: String): Boolean {
			val literal = IpAddress.parseLiteral(host) ?: return false
			if (literal.bytes.size != bytes.size) return false
			val masked = IpAddress.mask(literal.bytes, prefix)
			return masked.contentEquals(bytes)
		}

		override fun equals(other: Any?): Boolean =
			other is Ip && other.value == value

		override fun hashCode(): Int = value.hashCode()

		override fun toString(): String = value
	}

	companion object {
		/** Parse an entry as the user entered it. */
		fun parse(kind: AllowlistKind, input: String): AllowlistEntry {
			val text = input.trim()
			if (text.isEmpty()) throw AddressException("enter a host, IP address, or CIDR block")
			if (text.any { it.isWhitespace() }) throw AddressException("the entry must not contain whitespace")
			return when (kind) {
				AllowlistKind.DNS -> dns(text)
				AllowlistKind.IP -> ip(text, cidr = false)
				AllowlistKind.CIDR -> ip(text, cidr = true)
			}
		}

		/** Restore an entry from its serialized form, or `null` when it is invalid. */
		fun parseSerialized(raw: String): AllowlistEntry? {
			val separator = raw.indexOf(':')
			if (separator <= 0) return null
			val kind = AllowlistKind.fromId(raw.substring(0, separator)) ?: return null
			return runCatching { parse(kind, raw.substring(separator + 1)) }.getOrNull()
		}

		private fun dns(input: String): Dns {
			var pattern = input
			if (pattern.endsWith(".")) pattern = pattern.dropLast(1)
			if (pattern.isEmpty()) throw AddressException("DNS pattern is empty")
			if (IpAddress.parseLiteral(pattern) != null) {
				throw AddressException("add an IP address as an IP or CIDR entry instead")
			}
			if (pattern.any { it == '/' || it == '@' || it == '?' || it == '#' || it == ':' }) {
				throw AddressException("a DNS pattern cannot contain a path, port, or user info")
			}
			val rawLabels = pattern.split('.')
			if (rawLabels.any { it.isEmpty() }) throw AddressException("DNS pattern contains an empty label")
			var hasLiteral = false
			val regexLabels = rawLabels.map { label ->
				if (label == "**") return@map "**"
				if (label.contains("**")) {
					throw AddressException("'**' must be a whole label")
				}
				if (label.contains('*')) {
					if (label.any { it != '*' && !it.isLetterOrDigit() && it != '-' }) {
						throw AddressException("DNS pattern contains an invalid character")
					}
					if (label.any { it != '*' }) hasLiteral = true
					label.lowercase(Locale.ROOT)
				} else {
					hasLiteral = true
					val ascii = try {
						IDN.toASCII(label, IDN.ALLOW_UNASSIGNED)
					} catch (_: IllegalArgumentException) {
						throw AddressException("DNS pattern contains an invalid label")
					}
					val normalized = ascii.lowercase(Locale.ROOT)
					if (!labelPattern.matches(normalized)) {
						throw AddressException("DNS pattern contains an invalid label")
					}
					normalized
				}
			}
			if (!hasLiteral) {
				throw AddressException("a DNS pattern must contain at least one literal label")
			}
			val body = regexLabels.joinToString("\\.") { label ->
				if (label == "**") {
					"[^.]+(?:\\.[^.]+)*"
				} else {
					label.map { character ->
						if (character == '*') "[^.]+" else character.toString()
					}.joinToString("")
				}
			}
			val normalized = regexLabels.joinToString(".")
			return Dns(normalized, Regex("^$body$", RegexOption.IGNORE_CASE))
		}

		private fun ip(input: String, cidr: Boolean): Ip {
			val slash = input.indexOf('/')
			if (cidr && slash < 0) throw AddressException("a CIDR entry needs a '/' and a prefix length")
			if (!cidr && slash >= 0) throw AddressException("use a CIDR entry for a prefix length")
			if (input.count { it == '/' } > 1) {
				throw AddressException("invalid CIDR block")
			}
			val hostPart = if (slash >= 0) input.substring(0, slash) else input
			val literal = IpAddress.parseLiteral(hostPart)
				?: throw AddressException("enter a literal IPv4 or IPv6 address")
			val maxPrefix = literal.bytes.size * 8
			val prefix = if (slash >= 0) {
				val raw = input.substring(slash + 1)
				if (raw.isEmpty() || raw.any { it !in '0'..'9' }) {
					throw AddressException("the CIDR prefix must be a number")
				}
				val parsed = raw.toIntOrNull() ?: throw AddressException("the CIDR prefix is out of range")
				if (parsed !in 0..maxPrefix) {
					throw AddressException("the CIDR prefix must be between 0 and $maxPrefix")
				}
				parsed
			} else {
				maxPrefix
			}
			val masked = IpAddress.mask(literal.bytes, prefix)
			return Ip(IpAddress.canonical(masked, if (prefix == maxPrefix) null else prefix), masked, prefix)
		}

		private val labelPattern = Regex("^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$")
	}
}
