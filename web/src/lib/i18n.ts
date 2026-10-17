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
	attach_file: 'Attach file',
	upload_failed: 'Upload failed',
	download: 'Download',
	download_failed: 'Download failed',
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
	active: 'Active',
	profile: 'Profile',
	edit_profile: 'Edit profile',
	change_avatar: 'Change avatar',
	remove_avatar: 'Remove avatar',
	avatar_error: 'Could not read that image',
	profile_error: 'Could not update the profile',
	user_id_hint: 'Letters, digits, dot, underscore and hyphen; 3–32 characters.',
	cancel: 'Cancel',
	save: 'Save',
	incoming_requests: 'Incoming requests',
	outgoing_requests: 'Sent requests',
	reject: 'Reject',
	pending: 'Pending',
	chat: 'Chat',
	edit_contact: 'Edit contact',
	remark: 'Remark',
	remark_avatar: 'Remark avatar',
	contact_updated: 'Contact updated',
	new_group: 'New group',
	group_name: 'Group name',
	create_group: 'Create group',
	select_contacts: 'Select contacts',
	group_info: 'Group info',
	members: 'Members',
	member_count: '{count} members',
	add_members: 'Add members',
	leave_group: 'Leave group',
	dissolve_group: 'Dissolve group',
	rename_group: 'Rename',
	group_remark: 'Group remark',
	save_remark: 'Save remark',
	make_admin: 'Make admin',
	revoke_admin: 'Revoke admin',
	owner: 'Owner',
	admin: 'Admin',
	member: 'Member',
	kick: 'Remove',
	mute: 'Mute',
	unmute: 'Unmute',
	you: 'You',
	confirm_leave_group: 'Leave this group?',
	confirm_dissolve_group: 'Dissolve this group for everyone?',
	no_contacts_selected: 'Select at least one contact',
	group_error: 'Group action failed',
	contact_only_hint: 'Add someone as a contact before starting a direct chat.',
	select_contact: 'Select a contact',
	start_chat: 'Start chat',
	started_chat: 'Added and opened chat',
	no_blocks: 'No blocked users'
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
