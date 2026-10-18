import { get, writable } from 'svelte/store';

/**
 * Per-account absolute-time display settings.
 *
 * The time zone is either the browser's current system zone or a fixed offset
 * from UTC (in minutes), so users can keep a stable clock while travelling.
 * An empty format means "use the system's localized date and time format".
 */
export type TimeZoneSetting = 'system' | number;

export interface TimeSettings {
	timeZone: TimeZoneSetting;
	format: string;
}

export interface TimeZoneOption {
	value: string;
	label: string;
}

export const DEFAULT_TIME_SETTINGS: TimeSettings = {
	timeZone: 'system',
	format: ''
};

export const timeSettings = writable<TimeSettings>({ ...DEFAULT_TIME_SETTINGS });

const MIN_OFFSET = -12 * 60;
const MAX_OFFSET = 14 * 60;
const OFFSET_STEP = 15;

function storageKey(userId: number, name: string): string {
	return `chatbox.time.${userId}.${name}`;
}

/** Normalize persisted or user-entered values into a supported setting. */
export function normalizeTimeSettings(settings: Partial<TimeSettings>): TimeSettings {
	const zone = settings.timeZone;
	const timeZone: TimeZoneSetting =
		zone === 'system'
			? 'system'
			: typeof zone === 'number' &&
				  Number.isFinite(zone) &&
				  zone >= MIN_OFFSET &&
				  zone <= MAX_OFFSET &&
				  zone % OFFSET_STEP === 0
				? zone
				: 'system';
	const format = typeof settings.format === 'string' ? settings.format.trim() : '';
	return { timeZone, format };
}

/** Load the signed-in account's settings, falling back to the system defaults. */
export function loadTimeSettings(userId: number | null): void {
	if (userId === null || typeof localStorage === 'undefined') {
		timeSettings.set({ ...DEFAULT_TIME_SETTINGS });
		return;
	}
	const rawZone = localStorage.getItem(storageKey(userId, 'timeZone'));
	const rawFormat = localStorage.getItem(storageKey(userId, 'format'));
	const parsedZone =
		rawZone === null ? 'system' : rawZone === 'system' ? 'system' : Number(rawZone);
	timeSettings.set(
		normalizeTimeSettings({
			timeZone: parsedZone,
			format: rawFormat ?? ''
		})
	);
}

/** Persist settings for one account and apply them immediately. */
export function saveTimeSettings(userId: number | null, settings: Partial<TimeSettings>): void {
	const next = normalizeTimeSettings(settings);
	timeSettings.set(next);
	if (userId === null || typeof localStorage === 'undefined') return;
	localStorage.setItem(storageKey(userId, 'timeZone'), String(next.timeZone));
	localStorage.setItem(storageKey(userId, 'format'), next.format);
}

/** Remove the account overrides and return to the system defaults. */
export function resetTimeSettings(userId: number | null): void {
	timeSettings.set({ ...DEFAULT_TIME_SETTINGS });
	if (userId === null || typeof localStorage === 'undefined') return;
	localStorage.removeItem(storageKey(userId, 'timeZone'));
	localStorage.removeItem(storageKey(userId, 'format'));
}

export function offsetLabel(minutes: number): string {
	const sign = minutes < 0 ? '-' : '+';
	const absolute = Math.abs(minutes);
	const hours = Math.floor(absolute / 60)
		.toString()
		.padStart(2, '0');
	const rest = (absolute % 60).toString().padStart(2, '0');
	return `UTC${sign}${hours}:${rest}`;
}

export function systemOffsetLabel(now = new Date()): string {
	return offsetLabel(-now.getTimezoneOffset());
}

/** The options shown by the time-zone picker. */
export function timeZoneOptions(
	now = new Date(),
	systemLabel = 'System time zone'
): TimeZoneOption[] {
	const options: TimeZoneOption[] = [
		{ value: 'system', label: `${systemLabel} (${systemOffsetLabel(now)})` }
	];
	for (let minutes = MIN_OFFSET; minutes <= MAX_OFFSET; minutes += OFFSET_STEP) {
		options.push({ value: String(minutes), label: offsetLabel(minutes) });
	}
	return options;
}

function currentSettings(): TimeSettings {
	return get(timeSettings);
}

function shiftedDate(date: Date, timeZone: TimeZoneSetting): Date {
	return timeZone === 'system' ? date : new Date(date.getTime() + timeZone * 60_000);
}

function intlZone(timeZone: TimeZoneSetting): string | undefined {
	return timeZone === 'system' ? undefined : 'UTC';
}

function zonedParts(date: Date, timeZone: TimeZoneSetting) {
	const formatter = new Intl.DateTimeFormat('en-US', {
		timeZone: intlZone(timeZone),
		year: 'numeric',
		month: '2-digit',
		day: '2-digit',
		hour: '2-digit',
		minute: '2-digit',
		second: '2-digit',
		hourCycle: 'h23'
	});
	const values: Record<string, string> = {};
	for (const part of formatter.formatToParts(shiftedDate(date, timeZone))) {
		if (part.type !== 'literal') values[part.type] = part.value;
	}
	return {
		year: Number(values.year ?? '0'),
		month: Number(values.month ?? '1'),
		day: Number(values.day ?? '1'),
		hour: Number(values.hour ?? '0'),
		minute: Number(values.minute ?? '0'),
		second: Number(values.second ?? '0')
	};
}

function nameFor(
	date: Date,
	timeZone: TimeZoneSetting,
	options: Intl.DateTimeFormatOptions
): string {
	return new Intl.DateTimeFormat(undefined, { ...options, timeZone: intlZone(timeZone) }).format(
		shiftedDate(date, timeZone)
	);
}

function pad(value: number, length = 2): string {
	return value.toString().padStart(length, '0');
}

function offsetAt(date: Date, timeZone: TimeZoneSetting): number {
	return timeZone === 'system' ? -date.getTimezoneOffset() : timeZone;
}

function applyPattern(date: Date, settings: TimeSettings): string {
	const parts = zonedParts(date, settings.timeZone);
	const offset = offsetLabel(offsetAt(date, settings.timeZone));
	const tokenPattern =
		/\[[^\]]*\]|YYYY|YY|MMMM|MMM|MM|M|DD|D|dddd|ddd|HH|H|mm|m|ss|s|ZZ|Z/g;
	return settings.format.replace(tokenPattern, (token) => {
		switch (token) {
			case 'YYYY':
				return pad(parts.year, 4);
			case 'YY':
				return pad(parts.year % 100);
			case 'MMMM':
				return nameFor(date, settings.timeZone, { month: 'long' });
			case 'MMM':
				return nameFor(date, settings.timeZone, { month: 'short' });
			case 'MM':
				return pad(parts.month);
			case 'M':
				return String(parts.month);
			case 'DD':
				return pad(parts.day);
			case 'D':
				return String(parts.day);
			case 'dddd':
				return nameFor(date, settings.timeZone, { weekday: 'long' });
			case 'ddd':
				return nameFor(date, settings.timeZone, { weekday: 'short' });
			case 'HH':
				return pad(parts.hour);
			case 'H':
				return String(parts.hour);
			case 'mm':
				return pad(parts.minute);
			case 'm':
				return String(parts.minute);
			case 'ss':
				return pad(parts.second);
			case 's':
				return String(parts.second);
			case 'ZZ':
				return offset;
			case 'Z':
				return offset.replace(/^UTC/, '');
			default:
				return token.startsWith('[') ? token.slice(1, -1) : token;
		}
	});
}

/** Format an absolute timestamp using the account's time zone and pattern. */
export function formatTimestamp(
	value: string | Date,
	settings: TimeSettings = currentSettings()
): string {
	const date = value instanceof Date ? value : new Date(value);
	if (Number.isNaN(date.getTime())) return typeof value === 'string' ? value : '';
	if (settings.format) return applyPattern(date, settings);
	return new Intl.DateTimeFormat(undefined, {
		dateStyle: 'medium',
		timeStyle: 'short',
		timeZone: intlZone(settings.timeZone)
	}).format(shiftedDate(date, settings.timeZone));
}

/** Calendar-day key in the configured time zone, used for day separators. */
export function dayKey(value: string | Date, settings: TimeSettings = currentSettings()): string {
	const date = value instanceof Date ? value : new Date(value);
	if (Number.isNaN(date.getTime())) return 'invalid';
	const parts = zonedParts(date, settings.timeZone);
	return `${parts.year}-${pad(parts.month)}-${pad(parts.day)}`;
}

/** Localized date label for a day separator, using the configured time zone. */
export function formatDay(
	value: string,
	settings: TimeSettings,
	today: string,
	yesterday: string
): string {
	const date = new Date(value);
	if (Number.isNaN(date.getTime())) return value;
	const key = dayKey(date, settings);
	if (key === dayKey(new Date(), settings)) return today;
	if (key === dayKey(new Date(Date.now() - 86_400_000), settings)) return yesterday;
	return new Intl.DateTimeFormat(undefined, {
		year: 'numeric',
		month: 'short',
		day: 'numeric',
		timeZone: intlZone(settings.timeZone)
	}).format(shiftedDate(date, settings.timeZone));
}
