import { derived, get, writable } from 'svelte/store';
import {
	getActiveLocale,
	installedPlugins,
	languagePacks,
	setActiveLocale,
	type PluginPackage
} from './plugins';

/**
 * Client plugin: language packs.
 *
 * Only the base English dictionary is compiled into the client. Every other
 * language is installed at runtime from a plugin file and stored for the
 * signed-in user (see docs/plugin-system.md).
 */

export interface LanguageOption {
	code: string;
	label: string;
}

const baseStrings: Record<string, string> = {
	app_name: 'ChatBox',
	tagline: 'Fast, real-time conversations on every device.',
	login: 'Sign in',
	register: 'Create account',
	logout: 'Sign out',
	user_id: 'User ID',
	password: 'Password',
	display_name: 'Display name',
	send: 'Send',
	message_placeholder: 'Write a message…',
	conversations: 'Conversations',
	chats: 'Chats',
	contacts: 'Contacts',
	requests: 'Requests',
	search: 'Search',
	search_placeholder: 'Search messages…',
	blocks: 'Blocked users',
	no_conversations: 'No conversations yet',
	no_contacts: 'No contacts yet',
	no_messages: 'No messages yet — say hello!',
	select_conversation: 'Select a conversation to start chatting',
	new_conversation: 'New conversation',
	add_contact: 'Add a contact',
	accept: 'Accept',
	block: 'Block',
	unblock: 'Unblock',
	today: 'Today',
	yesterday: 'Yesterday',
	result: 'result',
	results: 'results',
	file: 'File',
	back: 'Back',
	close: 'Close',
	dismiss: 'Dismiss',
	language: 'Language',
	theme: 'Theme',
	light: 'Light',
	dark: 'Dark',
	system: 'System',
	online: 'Online',
	offline: 'Offline',
	connecting: 'Connecting…',
	no_account: 'No account?',
	have_account: 'Already registered?',
	plugins: 'Plugins',
	install_plugin: 'Install plugin',
	installed_plugins: 'Installed plugins',
	no_plugins: 'No plugins installed yet',
	remove: 'Remove',
	plugin_install_hint:
		'Choose a ChatBox client plugin file (.json). Plugins apply only to the current account.',
	plugin_file_too_large: 'Plugin file is too large (max 256 KiB)',
	plugin_installed: 'Plugin installed',
	plugin_removed: 'Plugin removed',
	plugin_error: 'Plugin installation failed',
	kind_language_pack: 'Language pack',
	kind_theme: 'Theme',
	active: 'Active'
};

export const locale = writable<string>('en');

export const languages = derived(installedPlugins, ($plugins: PluginPackage[]) => {
	const options: LanguageOption[] = [{ code: 'en', label: 'English' }];
	for (const pack of languagePacks($plugins)) {
		if (pack.payload.locale === 'en') continue;
		if (options.some((option) => option.code === pack.payload.locale)) continue;
		options.push({ code: pack.payload.locale, label: pack.payload.label });
	}
	return options;
});

export const translator = derived(
	[locale, installedPlugins],
	([$locale, $plugins]): ((key: string) => string) => {
		const pack = languagePacks($plugins).find((item) => item.payload.locale === $locale);
		return (key: string): string => pack?.payload.strings[key] ?? baseStrings[key] ?? key;
	}
);

/** Imperative helper for non-component modules. */
export function t(key: string): string {
	const current = get(locale);
	const pack = languagePacks(get(installedPlugins)).find(
		(item) => item.payload.locale === current
	);
	return pack?.payload.strings[key] ?? baseStrings[key] ?? key;
}

/** Activate a language and persist it for the current user. */
export function setLocale(userId: string | null, code: string): void {
	locale.set(code);
	if (userId) setActiveLocale(userId, code);
}

/** Restore the user's saved language, falling back to English. */
export function syncLocale(userId: string): void {
	const available = new Set(['en', ...languagePacks(get(installedPlugins)).map((p) => p.payload.locale)]);
	const stored = getActiveLocale(userId);
	const resolved = stored && available.has(stored) ? stored : 'en';
	locale.set(resolved);
	setActiveLocale(userId, resolved);
}

/** Reset the language after sign-out. */
export function resetLocale(): void {
	locale.set('en');
}
