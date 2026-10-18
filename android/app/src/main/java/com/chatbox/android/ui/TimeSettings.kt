package com.chatbox.android.ui

import android.content.Context
import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.time.format.TextStyle
import java.util.Locale

/** Absolute-time display settings for one account. */
data class TimeDisplaySettings(
	/** `system` or a fixed offset from UTC in minutes, stored as a string. */
	val timeZone: String = "system",
	/** Empty means the system's localized date and time format. */
	val format: String = "",
)

data class TimeZoneOption(val value: String, val label: String)

/** Persists time display settings per account, locally on the device. */
class TimeSettingsStore(context: Context) {
	private val prefs = context.getSharedPreferences("chatbox.time", Context.MODE_PRIVATE)

	fun load(userId: Long): TimeDisplaySettings {
		val zone = prefs.getString(key(userId, "timeZone"), "system") ?: "system"
		val format = prefs.getString(key(userId, "format"), "") ?: ""
		return normalizeTimeSettings(TimeDisplaySettings(zone, format))
	}

	fun save(userId: Long, settings: TimeDisplaySettings) {
		val next = normalizeTimeSettings(settings)
		prefs.edit()
			.putString(key(userId, "timeZone"), next.timeZone)
			.putString(key(userId, "format"), next.format)
			.apply()
	}

	fun reset(userId: Long) {
		prefs.edit()
			.remove(key(userId, "timeZone"))
			.remove(key(userId, "format"))
			.apply()
	}

	private fun key(userId: Long, name: String): String = "$name:$userId"
}

fun normalizeTimeSettings(settings: TimeDisplaySettings): TimeDisplaySettings {
	val zone = settings.timeZone.toIntOrNull()
	val normalizedZone = when {
		settings.timeZone == "system" -> "system"
		zone != null && zone in -720..840 && zone % 15 == 0 -> zone.toString()
		else -> "system"
	}
	return TimeDisplaySettings(normalizedZone, settings.format.trim())
}

object TimeFormat {
	fun zone(value: String): ZoneId {
		val minutes = value.toIntOrNull() ?: return ZoneId.systemDefault()
		return ZoneOffset.ofTotalSeconds(minutes * 60)
	}

	/** Format an RFC 3339 timestamp, falling back to the raw value on error. */
	fun format(createdAt: String, settings: TimeDisplaySettings): String {
		val instant = runCatching { Instant.parse(createdAt) }.getOrNull() ?: return createdAt
		val zoned = instant.atZone(zone(settings.timeZone))
		if (settings.format.isBlank()) {
			return DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT)
				.withZone(zone(settings.timeZone))
				.format(instant)
		}
		return applyPattern(zoned, settings.format)
	}

	fun timeZoneOptions(systemLabel: String, now: ZonedDateTime = ZonedDateTime.now()): List<TimeZoneOption> {
		val options = mutableListOf(
			TimeZoneOption("system", "$systemLabel (${offsetLabel(now.offset.totalSeconds / 60)})"),
		)
		for (minutes in -12 * 60..14 * 60 step 15) {
			options += TimeZoneOption(minutes.toString(), offsetLabel(minutes))
		}
		return options
	}

	private fun applyPattern(zoned: ZonedDateTime, pattern: String): String {
		val output = StringBuilder()
		var index = 0
		while (index < pattern.length) {
			if (pattern[index] == '[') {
				val close = pattern.indexOf(']', index + 1)
				if (close >= 0) {
					output.append(pattern, index + 1, close)
					index = close + 1
					continue
				}
			}
			val token = TOKENS.firstOrNull { pattern.startsWith(it, index) }
			if (token == null) {
				output.append(pattern[index])
				index += 1
				continue
			}
			output.append(
				when (token) {
					"YYYY" -> zoned.year.toString().padStart(4, '0')
					"YY" -> (zoned.year % 100).toString().padStart(2, '0')
					"MMMM" -> zoned.month.getDisplayName(TextStyle.FULL, Locale.getDefault())
					"MMM" -> zoned.month.getDisplayName(TextStyle.SHORT, Locale.getDefault())
					"MM" -> zoned.monthValue.toString().padStart(2, '0')
					"M" -> zoned.monthValue.toString()
					"DD" -> zoned.dayOfMonth.toString().padStart(2, '0')
					"D" -> zoned.dayOfMonth.toString()
					"dddd" -> zoned.dayOfWeek.getDisplayName(TextStyle.FULL, Locale.getDefault())
					"ddd" -> zoned.dayOfWeek.getDisplayName(TextStyle.SHORT, Locale.getDefault())
					"HH" -> zoned.hour.toString().padStart(2, '0')
					"H" -> zoned.hour.toString()
					"mm" -> zoned.minute.toString().padStart(2, '0')
					"m" -> zoned.minute.toString()
					"ss" -> zoned.second.toString().padStart(2, '0')
					"s" -> zoned.second.toString()
					"ZZ" -> offsetLabel(zoned.offset.totalSeconds / 60)
					"Z" -> offsetString(zoned.offset.totalSeconds / 60)
					else -> token
				},
			)
			index += token.length
		}
		return output.toString()
	}

	private val TOKENS = listOf(
		"YYYY", "YY", "MMMM", "MMM", "MM", "M", "DD", "D", "dddd", "ddd",
		"HH", "H", "mm", "m", "ss", "s", "ZZ", "Z",
	)
}

fun offsetString(minutes: Int): String {
	val sign = if (minutes < 0) '-' else '+'
	val absolute = kotlin.math.abs(minutes)
	return "$sign${(absolute / 60).toString().padStart(2, '0')}:${(absolute % 60).toString().padStart(2, '0')}"
}

fun offsetLabel(minutes: Int): String = "UTC${offsetString(minutes)}"
