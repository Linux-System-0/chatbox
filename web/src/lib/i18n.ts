import { derived, writable } from 'svelte/store';

/**
 * Client plugin: language packs.
 *
 * Translations are a client plugin per docs/plugin-system.md: they are
 * installed and applied entirely on the client and never affect the server.
 */

export interface LanguageOption {
	code: string;
	label: string;
}

export const languages: LanguageOption[] = [
	{ code: 'en', label: 'English' },
	{ code: 'zh-CN', label: '简体中文' }
];

const builtin: Record<string, Record<string, string>> = {
	en: {
		app_name: 'ChatBox',
		login: 'Sign in',
		register: 'Create account',
		logout: 'Sign out',
		user_id: 'User ID',
		password: 'Password',
		display_name: 'Display name',
		send: 'Send',
		message_placeholder: 'Write a message…',
		conversations: 'Conversations',
		contacts: 'Contacts',
		search: 'Search',
		search_placeholder: 'Search messages…',
		blocks: 'Blocked users',
		no_conversations: 'No conversations yet',
		language: 'Language',
		theme: 'Theme',
		light: 'Light',
		dark: 'Dark',
		online: 'Online',
		offline: 'Offline',
		connecting: 'Connecting…'
	},
	'zh-CN': {
		app_name: 'ChatBox',
		login: '登录',
		register: '注册',
		logout: '退出登录',
		user_id: '用户 ID',
		password: '密码',
		display_name: '显示名称',
		send: '发送',
		message_placeholder: '输入消息…',
		conversations: '会话',
		contacts: '联系人',
		search: '搜索',
		search_placeholder: '搜索消息…',
		blocks: '已拉黑用户',
		no_conversations: '暂无会话',
		language: '语言',
		theme: '主题',
		light: '浅色',
		dark: '深色',
		online: '在线',
		offline: '离线',
		connecting: '连接中…'
	}
};

const STORAGE_KEY = 'chatbox.locale';

export const locale = writable<string>(
	(typeof localStorage !== 'undefined' && localStorage.getItem(STORAGE_KEY)) || 'en'
);

locale.subscribe((value) => {
	if (typeof localStorage !== 'undefined') localStorage.setItem(STORAGE_KEY, value);
});

export const translator = derived(locale, (value) => {
	const table = builtin[value] ?? builtin.en;
	return (key: string): string => table[key] ?? builtin.en[key] ?? key;
});

/** Imperative helper for non-component modules. */
export function t(key: string, value?: string): string {
	const table = builtin[value ?? 'en'] ?? builtin.en;
	return table[key] ?? builtin.en[key] ?? key;
}
