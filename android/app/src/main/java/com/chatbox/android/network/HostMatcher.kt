package com.chatbox.android.network

/** Host matching shared by the policy store and the request interceptor. */
object HostMatcher {

	/**
	 * Normalize a host for matching: lowercase, IDNA/punycode, no trailing dot.
	 *
	 * Host names are normalized with the same rules as a server address, but a
	 * literal IP is kept in its canonical text form without brackets.
	 */
	fun normalizeHost(host: String): String = ServerAddress.normalizeHost(host)

	/** Whether the host is an IPv4 or IPv6 literal rather than a DNS name. */
	fun isLiteralHost(host: String): Boolean = IpAddress.parseLiteral(host) != null

	fun matches(entry: AllowlistEntry, host: String): Boolean {
		val normalized = runCatching { normalizeHost(host) }.getOrNull() ?: return false
		return when (entry) {
			is AllowlistEntry.Dns -> entry.matches(normalized)
			is AllowlistEntry.Ip -> entry.contains(normalized)
		}
	}
}
