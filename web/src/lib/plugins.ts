import { writable } from 'svelte/store';

/**
 * Runtime client plugins.
 *
 * A plugin is a single self-contained JSON file that the user installs from
 * their device. It is stored locally and scoped to the signed-in user; the file
 * is never sent to the server (see docs/plugin-system.md).
 */

export const PLUGIN_API_VERSION = 1;
export const MAX_PLUGIN_BYTES = 256 * 1024;

export type PluginKind = 'language-pack' | 'theme';

export interface LanguagePayload {
	locale: string;
	label: string;
	strings: Record<string, string>;
}

export interface ThemePayload {
	id: string;
	label: string;
	dark: boolean;
	colors: Record<string, string>;
}

export interface PluginPackage {
	name: string;
	version: string;
	kind: PluginKind;
	api_version: number;
	description: string;
	payload: LanguagePayload | ThemePayload;
}

export type LanguagePlugin = PluginPackage & {
	kind: 'language-pack';
	payload: LanguagePayload;
};

export type ThemePlugin = PluginPackage & {
	kind: 'theme';
	payload: ThemePayload;
};

const NAME_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;
const LOCALE_PATTERN = /^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*$/;
const COLOR_PATTERN = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;
const MAX_STRINGS = 2000;
const MAX_STRING_LENGTH = 4000;
const RESERVED_THEME_IDS = new Set(['system', 'light', 'dark']);

export const THEME_COLOR_KEYS = new Set([
	'background',
	'surface',
	'surfaceAlt',
	'text',
	'muted',
	'border',
	'primary',
	'primaryHover',
	'onPrimary',
	'primarySoft',
	'bubbleIn',
	'bubbleOut',
	'bubbleOutText'
]);

/** Installed plugins for the currently loaded user. */
export const installedPlugins = writable<PluginPackage[]>([]);

let loadedUserId: string | null = null;

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function requireString(value: unknown, field: string): string {
	if (typeof value !== 'string' || value.trim() === '') {
		throw new Error(`${field} must be a non-empty string`);
	}
	return value;
}

function parseLanguagePayload(value: unknown): LanguagePayload {
	if (!isRecord(value)) throw new Error('payload must be an object');
	const locale = requireString(value.locale, 'payload.locale');
	if (!LOCALE_PATTERN.test(locale)) throw new Error('payload.locale is invalid');
	const label = requireString(value.label, 'payload.label');
	if (!isRecord(value.strings)) throw new Error('payload.strings must be an object');
	const entries = Object.entries(value.strings);
	if (entries.length === 0) throw new Error('payload.strings must not be empty');
	if (entries.length > MAX_STRINGS) throw new Error('payload.strings has too many entries');
	const strings: Record<string, string> = Object.create(null);
	for (const [key, raw] of entries) {
		if (key.trim() === '') throw new Error('payload.strings keys must not be empty');
		if (typeof raw !== 'string') throw new Error(`payload.strings.${key} must be a string`);
		if (raw.length > MAX_STRING_LENGTH) throw new Error(`payload.strings.${key} is too long`);
		strings[key] = raw;
	}
	return { locale, label, strings };
}

function parseThemePayload(value: unknown): ThemePayload {
	if (!isRecord(value)) throw new Error('payload must be an object');
	const id = requireString(value.id, 'payload.id');
	if (!NAME_PATTERN.test(id)) throw new Error('payload.id is invalid');
	if (RESERVED_THEME_IDS.has(id)) throw new Error('payload.id is reserved');
	const label = requireString(value.label, 'payload.label');
	if (value.dark !== undefined && typeof value.dark !== 'boolean') {
		throw new Error('payload.dark must be a boolean');
	}
	if (!isRecord(value.colors)) throw new Error('payload.colors must be an object');
	const colors: Record<string, string> = Object.create(null);
	for (const [key, raw] of Object.entries(value.colors)) {
		if (!THEME_COLOR_KEYS.has(key)) continue;
		const color = requireString(raw, `payload.colors.${key}`);
		if (!COLOR_PATTERN.test(color)) {
			throw new Error(`payload.colors.${key} must be a hex color`);
		}
		colors[key] = color;
	}
	if (Object.keys(colors).length === 0) {
		throw new Error('payload.colors must contain at least one known color');
	}
	return { id, label, dark: value.dark === true, colors };
}

/** Parse and validate a plugin file. Throws on malformed input. */
export function parsePlugin(text: string): PluginPackage {
	if (typeof text !== 'string') throw new Error('plugin file must be text');
	const byteLength = new TextEncoder().encode(text).length;
	if (byteLength > MAX_PLUGIN_BYTES) {
		throw new Error('plugin file is too large (max 256 KiB)');
	}
	let raw: unknown;
	try {
		raw = JSON.parse(text);
	} catch {
		throw new Error('plugin file is not valid JSON');
	}
	if (!isRecord(raw)) throw new Error('plugin file must contain a JSON object');

	const name = requireString(raw.name, 'name');
	if (!NAME_PATTERN.test(name)) throw new Error('name is invalid');
	const version = requireString(raw.version, 'version');
	const kind = requireString(raw.kind, 'kind');
	if (kind !== 'language-pack' && kind !== 'theme') {
		throw new Error("kind must be 'language-pack' or 'theme'");
	}
	if (raw.api_version !== PLUGIN_API_VERSION) {
		throw new Error(
			`unsupported api_version ${String(raw.api_version)} (expected ${PLUGIN_API_VERSION})`
		);
	}
	let description = '';
	if (raw.description !== undefined) {
		if (typeof raw.description !== 'string') throw new Error('description must be a string');
		description = raw.description;
	}

	if (kind === 'language-pack') {
		return {
			name,
			version,
			kind,
			api_version: PLUGIN_API_VERSION,
			description,
			payload: parseLanguagePayload(raw.payload)
		};
	}
	return {
		name,
		version,
		kind,
		api_version: PLUGIN_API_VERSION,
		description,
		payload: parseThemePayload(raw.payload)
	};
}

export function isLanguagePlugin(plugin: PluginPackage): plugin is LanguagePlugin {
	return plugin.kind === 'language-pack';
}

export function isThemePlugin(plugin: PluginPackage): plugin is ThemePlugin {
	return plugin.kind === 'theme';
}

export function languagePacks(plugins: PluginPackage[]): LanguagePlugin[] {
	return plugins.filter(isLanguagePlugin);
}

export function themePacks(plugins: PluginPackage[]): ThemePlugin[] {
	return plugins.filter(isThemePlugin);
}

function storageAvailable(): boolean {
	return typeof localStorage !== 'undefined';
}

function pluginsKey(userId: string): string {
	return `chatbox.plugins.v1.${encodeURIComponent(userId)}`;
}

function localeKey(userId: string): string {
	return `chatbox.locale.v1.${encodeURIComponent(userId)}`;
}

function themeKey(userId: string): string {
	return `chatbox.theme.v1.${encodeURIComponent(userId)}`;
}

function readInstalled(userId: string): PluginPackage[] {
	if (!storageAvailable()) return [];
	let raw: string | null = null;
	try {
		raw = localStorage.getItem(pluginsKey(userId));
	} catch {
		return [];
	}
	if (!raw) return [];
	try {
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return [];
		const plugins: PluginPackage[] = [];
		for (const item of parsed) {
			try {
				plugins.push(parsePlugin(JSON.stringify(item)));
			} catch {
				// Drop invalid or outdated entries instead of breaking the client.
			}
		}
		return plugins.sort((a, b) => a.name.localeCompare(b.name));
	} catch {
		return [];
	}
}

function writeInstalled(userId: string, plugins: PluginPackage[]): void {
	if (!storageAvailable()) return;
	localStorage.setItem(pluginsKey(userId), JSON.stringify(plugins));
}

/** Load the signed-in user's installed plugins into the store. */
export function loadPluginsFor(userId: string): PluginPackage[] {
	const plugins = readInstalled(userId);
	loadedUserId = userId;
	installedPlugins.set(plugins);
	return plugins;
}

/** Drop in-memory plugins when the user signs out. */
export function clearPlugins(): void {
	loadedUserId = null;
	installedPlugins.set([]);
}

/**
 * Move per-user local data when the account changes its custom user ID.
 *
 * Plugins, the active language, and the active theme are keyed by the public
 * user ID in localStorage, so an ID change must carry those preferences over.
 */
export function migrateUserStorage(oldUserId: string, newUserId: string): void {
	if (!storageAvailable() || oldUserId === newUserId) return;
	for (const key of [pluginsKey, localeKey, themeKey]) {
		try {
			const from = key(oldUserId);
			const to = key(newUserId);
			const value = localStorage.getItem(from);
			if (value !== null && localStorage.getItem(to) === null) {
				localStorage.setItem(to, value);
			}
			localStorage.removeItem(from);
		} catch {
			// Preference migration is best-effort.
		}
	}
	if (loadedUserId === oldUserId) loadedUserId = newUserId;
}

/** Install or replace a plugin for `userId`; returns the parsed package. */
export function installPlugin(userId: string, text: string): PluginPackage {
	const plugin = parsePlugin(text);
	const next = readInstalled(userId).filter((item) => item.name !== plugin.name);
	next.push(plugin);
	next.sort((a, b) => a.name.localeCompare(b.name));
	writeInstalled(userId, next);
	if (loadedUserId === userId) installedPlugins.set(next);
	return plugin;
}

/** Remove one plugin belonging to `userId`. */
export function removePlugin(userId: string, name: string): void {
	const next = readInstalled(userId).filter((item) => item.name !== name);
	writeInstalled(userId, next);
	if (loadedUserId === userId) installedPlugins.set(next);
}

export function getActiveLocale(userId: string): string | null {
	if (!storageAvailable()) return null;
	try {
		return localStorage.getItem(localeKey(userId));
	} catch {
		return null;
	}
}

export function setActiveLocale(userId: string, locale: string): void {
	if (!storageAvailable()) return;
	try {
		localStorage.setItem(localeKey(userId), locale);
	} catch {
		// Losing the active-language preference is not fatal.
	}
}

export function getActiveTheme(userId: string): string | null {
	if (!storageAvailable()) return null;
	try {
		return localStorage.getItem(themeKey(userId));
	} catch {
		return null;
	}
}

export function setActiveTheme(userId: string, theme: string): void {
	if (!storageAvailable()) return;
	try {
		localStorage.setItem(themeKey(userId), theme);
	} catch {
		// Losing the active-theme preference is not fatal.
	}
}
