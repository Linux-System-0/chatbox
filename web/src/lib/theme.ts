import { writable } from 'svelte/store';

/**
 * Client plugin: UI theme.
 *
 * As with language packs, themes are installed and applied on the client only
 * and have no server-side effect (see docs/plugin-system.md).
 */

export type Theme = 'light' | 'dark';

const STORAGE_KEY = 'chatbox.theme';

function initial(): Theme {
	if (typeof localStorage === 'undefined') return 'light';
	return (localStorage.getItem(STORAGE_KEY) as Theme) ?? 'light';
}

export const theme = writable<Theme>(initial());

export function applyTheme(value: Theme): void {
	if (typeof document !== 'undefined') {
		document.documentElement.dataset.theme = value;
	}
	if (typeof localStorage !== 'undefined') {
		localStorage.setItem(STORAGE_KEY, value);
	}
	theme.set(value);
}
