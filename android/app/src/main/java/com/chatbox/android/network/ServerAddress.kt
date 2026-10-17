package com.chatbox.android.network

import java.net.IDN
import java.util.Locale

/** Invalid user input for a server address or a policy entry. */
class AddressException(message: String) : IllegalArgumentException(message)

/** A validated, normalized server address (`scheme://host[:port]`). */
data class ServerAddress(
	val scheme: Scheme,
	val host: String,
	val port: Int? = null,
) {
	enum class Scheme(val wire: String, val defaultPort: Int) {
		HTTP("http", 80),
		HTTPS("https", 443),
		;

		companion object {
			fun fromWire(value: String): Scheme? =
				entries.firstOrNull { it.wire == value.lowercase(Locale.ROOT) }
		}
	}

	val effectivePort: Int get() = port ?: scheme.defaultPort

	val isCleartext: Boolean get() = scheme == Scheme.HTTP

	/** The normalized origin; the port is omitted when it is the scheme default. */
	val origin: String
		get() = buildString {
			append(scheme.wire)
			append("://")
			append(host)
			if (port != null && port != scheme.defaultPort) {
				append(':')
				append(port)
			}
		}

	companion object {
		private val labelPattern = Regex("^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$")

		/** Parse user input: `host[:port]`, optionally prefixed with a scheme. */
		fun parse(input: String, scheme: Scheme = Scheme.HTTP): ServerAddress {
			if (input.isEmpty()) throw AddressException("enter a server address")
			if (input.any { it.isWhitespace() }) {
				throw AddressException("a server address must not contain whitespace")
			}
			var text = input
			var selected = scheme
			when {
				text.startsWith("http://", ignoreCase = true) -> {
					selected = Scheme.HTTP
					text = text.substring("http://".length)
				}

				text.startsWith("https://", ignoreCase = true) -> {
					selected = Scheme.HTTPS
					text = text.substring("https://".length)
				}
			}
			if (text.isEmpty()) throw AddressException("enter a server address")
			if (text.any { it == '/' || it == '\\' || it == '?' || it == '#' || it == '@' }) {
				throw AddressException("a server address cannot contain a path, query, fragment, or user info")
			}
			val (host, port) = splitHostAndPort(text)
			return ServerAddress(selected, host, port)
		}

		/** Build a normalized address from URL parts. */
		fun of(scheme: String, host: String, port: Int): ServerAddress {
			val selected = Scheme.fromWire(scheme)
				?: throw AddressException("unsupported scheme '$scheme'")
			val normalized = when {
				host.startsWith("[") && host.endsWith("]") -> normalizeIpv6(host.substring(1, host.length - 1))
				host.contains(':') -> normalizeIpv6(host)
				IpAddress.parseV4(host) != null -> IpAddress.canonical(IpAddress.parseV4(host)!!)
				else -> normalizeHostName(host)
			}
			return ServerAddress(selected, normalized, port.takeIf { it != selected.defaultPort })
		}

		/** Normalize a host for policy matching (lowercase, IDNA, no trailing dot). */
		fun normalizeHost(raw: String): String {
			val unbracketed = raw.removeSurrounding("[", "]")
			if (unbracketed.isEmpty()) throw AddressException("host is empty")
			val literal = IpAddress.parseLiteral(unbracketed)
			if (literal != null) {
				return IpAddress.canonical(literal.bytes)
			}
			var text = unbracketed
			if (text.contains(':')) throw AddressException("invalid IPv6 literal")
			if (text.endsWith(".")) text = text.dropLast(1)
			if (text.isEmpty()) throw AddressException("host is empty")
			if (text.any { it == '*' }) throw AddressException("a server address cannot contain a wildcard")
			val ascii = try {
				IDN.toASCII(text, IDN.ALLOW_UNASSIGNED)
			} catch (_: IllegalArgumentException) {
				throw AddressException("invalid host name")
			}
			val normalized = ascii.lowercase(Locale.ROOT)
			validateHostName(normalized)
			return normalized
		}

		private fun splitHostAndPort(text: String): Pair<String, Int?> {
			if (text.startsWith("[")) {
				val end = text.indexOf(']')
				if (end < 2) throw AddressException("invalid IPv6 address")
				val literal = text.substring(1, end)
				val rest = text.substring(end + 1)
				val port = when {
					rest.isEmpty() -> null
					rest.startsWith(':') -> parsePort(rest.substring(1))
					else -> throw AddressException("unexpected characters after the IPv6 host")
				}
				return normalizeIpv6(literal) to port
			}
			val colons = text.count { it == ':' }
			if (colons > 1) {
				// A bare IPv6 literal is accepted and bracketed, but cannot carry a port.
				return normalizeIpv6(text) to null
			}
			if (colons == 1) {
				val index = text.indexOf(':')
				val host = text.substring(0, index)
				if (host.isEmpty()) throw AddressException("host is empty")
				return normalizeHostName(host) to parsePort(text.substring(index + 1))
			}
			if (text.isEmpty()) throw AddressException("host is empty")
			return normalizeHostName(text) to null
		}

		private fun parsePort(raw: String): Int {
			if (raw.isEmpty()) throw AddressException("port is empty")
			if (raw.any { it !in '0'..'9' }) throw AddressException("port must be a number")
			val port = raw.toIntOrNull() ?: throw AddressException("port is out of range")
			if (port !in 1..65535) throw AddressException("port must be between 1 and 65535")
			return port
		}

		private fun normalizeIpv6(raw: String): String {
			val bytes = IpAddress.parseV6(raw) ?: throw AddressException("invalid IPv6 address")
			return "[${IpAddress.formatV6(bytes)}]"
		}

		private fun normalizeHostName(raw: String): String {
			var text = raw
			if (text.isEmpty()) throw AddressException("host is empty")
			if (text.endsWith(".")) text = text.dropLast(1)
			if (text.isEmpty()) throw AddressException("host is empty")
			if (text.any { it == '*' }) throw AddressException("a server address cannot contain a wildcard")
			IpAddress.parseV4(text)?.let { return IpAddress.formatV4(it) }
			val numericLabels = text.split('.')
			if (numericLabels.size == 4 && numericLabels.all { label ->
					label.isNotEmpty() && label.all { it in '0'..'9' }
				}
			) {
				throw AddressException("invalid IPv4 address")
			}
			val ascii = try {
				IDN.toASCII(text, IDN.ALLOW_UNASSIGNED)
			} catch (_: IllegalArgumentException) {
				throw AddressException("invalid host name")
			}
			val normalized = ascii.lowercase(Locale.ROOT)
			validateHostName(normalized)
			return normalized
		}

		private fun validateHostName(host: String) {
			if (host.length > 253) throw AddressException("host name is too long")
			val labels = host.split('.')
			if (labels.any { !labelPattern.matches(it) }) throw AddressException("invalid host name")
		}
	}
}
