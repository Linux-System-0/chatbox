package com.chatbox.android.network

/**
 * Literal IP address parsing and formatting.
 *
 * The JDK's `InetAddress` resolves names and formats IPv6 addresses without
 * zero compression, so the network policy uses its own literal-only parser to
 * keep normalization deterministic and to avoid any DNS lookup while matching
 * a policy entry.
 */
internal object IpAddress {

	sealed interface Parsed {
		val bytes: ByteArray
		val prefixLength: Int

		data class V4(override val bytes: ByteArray) : Parsed {
			override val prefixLength: Int get() = 32
		}

		data class V6(override val bytes: ByteArray) : Parsed {
			override val prefixLength: Int get() = 128
		}
	}

	/** Parse an IPv4 or IPv6 literal, or return `null` when the text is not one. */
	fun parseLiteral(text: String): Parsed? {
		val raw = text.removeSurrounding("[", "]")
		if (raw.isEmpty()) return null
		if (raw.any { it.isWhitespace() } || raw.contains('%')) return null
		return if (raw.contains(':')) parseV6(raw)?.let { Parsed.V6(it) } else parseV4(raw)?.let { Parsed.V4(it) }
	}

	fun parseV4(text: String): ByteArray? {
		val parts = text.split('.')
		if (parts.size != 4) return null
		val bytes = ByteArray(4)
		for ((index, part) in parts.withIndex()) {
			if (part.isEmpty() || part.length > 3) return null
			if (part.length > 1 && part[0] == '0') return null
			if (!part.all { it in '0'..'9' }) return null
			val value = part.toInt()
			if (value > 255) return null
			bytes[index] = value.toByte()
		}
		return bytes
	}

	fun parseV6(text: String): ByteArray? {
		if (!text.contains(':')) return null
		var head = text
		var tail = ""
		val doubleColon = text.indexOf("::")
		if (doubleColon >= 0) {
			if (text.indexOf("::", doubleColon + 1) >= 0) return null
			head = text.substring(0, doubleColon)
			tail = text.substring(doubleColon + 2)
		}
		val headGroups = groups(head) ?: return null
		val tailGroups = groups(tail) ?: return null
		val total = headGroups.size + tailGroups.size
		val bytes = ByteArray(16)
		if (doubleColon < 0) {
			if (total != 8) return null
			headGroups.forEachIndexed { index, group -> writeGroup(bytes, index, group) }
		} else {
			if (total > 7) return null
			headGroups.forEachIndexed { index, group -> writeGroup(bytes, index, group) }
			val offset = 8 - tailGroups.size
			tailGroups.forEachIndexed { index, group -> writeGroup(bytes, offset + index, group) }
		}
		return bytes
	}

	private fun groups(text: String): List<Int>? {
		if (text.isEmpty()) return emptyList()
		val parts = text.split(':')
		if (parts.any { it.isEmpty() }) return null
		val result = mutableListOf<Int>()
		for ((index, part) in parts.withIndex()) {
			if (index == parts.lastIndex && part.contains('.')) {
				val v4 = parseV4(part) ?: return null
				result += ((v4[0].toInt() and 0xFF) shl 8) or (v4[1].toInt() and 0xFF)
				result += ((v4[2].toInt() and 0xFF) shl 8) or (v4[3].toInt() and 0xFF)
				continue
			}
			if (part.length > 4) return null
			if (!part.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) return null
			result += part.toInt(16)
		}
		return result
	}

	private fun writeGroup(bytes: ByteArray, index: Int, value: Int) {
		bytes[index * 2] = (value shr 8).toByte()
		bytes[index * 2 + 1] = value.toByte()
	}

	fun formatV4(bytes: ByteArray): String = bytes.joinToString(".") { (it.toInt() and 0xFF).toString() }

	/** Format 16 bytes as a compressed, lowercase RFC 5952 IPv6 literal. */
	fun formatV6(bytes: ByteArray): String {
		require(bytes.size == 16) { "IPv6 address must have 16 bytes" }
		val groups = IntArray(8) { index ->
			((bytes[index * 2].toInt() and 0xFF) shl 8) or (bytes[index * 2 + 1].toInt() and 0xFF)
		}
		var bestStart = -1
		var bestLength = 0
		var index = 0
		while (index < groups.size) {
			if (groups[index] != 0) {
				index++
				continue
			}
			val start = index
			while (index < groups.size && groups[index] == 0) index++
			val length = index - start
			if (length > bestLength) {
				bestStart = start
				bestLength = length
			}
		}
		if (bestLength < 2) bestStart = -1
		val text = StringBuilder()
		index = 0
		while (index < groups.size) {
			if (index == bestStart) {
				text.append("::")
				index += bestLength
				continue
			}
			if (text.isNotEmpty() && !text.endsWith(":")) text.append(':')
			text.append(Integer.toHexString(groups[index]))
			index++
		}
		return text.toString()
	}

	/** Least-significant bits of [prefix] set to zero. */
	fun mask(bytes: ByteArray, prefix: Int): ByteArray {
		val limited = prefix.coerceIn(0, bytes.size * 8)
		return ByteArray(bytes.size) { index ->
			val bits = (limited - index * 8).coerceIn(0, 8)
			val mask = if (bits == 0) 0 else (0xFF shl (8 - bits)) and 0xFF
			(bytes[index].toInt() and mask).toByte()
		}
	}

	fun canonical(bytes: ByteArray, prefix: Int? = null): String {
		val address = if (bytes.size == 4) formatV4(bytes) else formatV6(bytes)
		return if (prefix == null) address else "$address/$prefix"
	}
}
