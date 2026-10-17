import { derived, get, writable } from 'svelte/store';
import {
	getActiveTheme,
	installedPlugins,
	setActiveTheme,
	themePacks,
	type PluginPackage
} from './plugins';

/**
 * Client plugin: UI theme.
 *
 * The built-in light/dark schemes live in CSS; an installed theme package
 * overrides the design tokens at runtime for the signed-in user. No theme is
 * compiled into the bundle (see docs/plugin-system.md).
 */

export interface ThemeOption {
	id: string;
	label: string;
	/** Built-in label is a translation key; plugin labels are shown verbatim. */
	builtin: boolean;
}

/** Map a theme package color key to the CSS custom properties it sets. */
const THEME_VARS: Record<string, string[]> = {
	background: ['--bg'],
	surface: ['--surface'],
	surfaceAlt: ['--surface-2', '--surface-3'],
	text: ['--text'],
	muted: ['--muted'],
	border: ['--border'],
	primary: ['--accent'],
	primaryHover: ['--accent-hover'],
	onPrimary: ['--accent-contrast'],
	primarySoft: ['--accent-soft'],
	bubbleIn: ['--bubble-in'],
	bubbleOut: ['--bubble-out'],
	bubbleOutText: ['--bubble-out-text']
};

const BASE_THEMES: ThemeOption[] = [
	{ id: 'system', label: 'system', builtin: true },
	{ id: 'light', label: 'light', builtin: true },
	{ id: 'dark', label: 'dark', builtin: true }
];

function initialTheme(): string {
	if (typeof localStorage === 'undefined') return 'light';
	try {
		const stored = localStorage.getItem('chatbox.theme');
		return stored === 'dark' || stored === 'light' ? stored : 'light';
	} catch {
		return 'light';
	}
}

export const theme = writable<string>(initialTheme());

export const themes = derived(installedPlugins, ($plugins: PluginPackage[]) => {
	const options = [...BASE_THEMES];
	for (const pack of themePacks($plugins)) {
		if (options.some((option) => option.id === pack.payload.id)) continue;
		options.push({ id: pack.payload.id, label: pack.payload.label, builtin: false });
	}
	return options;
});

let appliedVars: string[] = [];

function clearOverrides(): void {
	for (const name of appliedVars) {
		document.documentElement.style.removeProperty(name);
	}
	appliedVars = [];
}

function systemPrefersDark(): boolean {
	return (
		typeof window !== 'undefined' &&
		typeof window.matchMedia === 'function' &&
		window.matchMedia('(prefers-color-scheme: dark)').matches
	);
}

/** Apply a built-in scheme id or an installed theme package id. */
export function applyTheme(value: string): void {
	theme.set(value);
	if (typeof document === 'undefined') return;

	clearOverrides();
	const pack = themePacks(get(installedPlugins)).find((item) => item.payload.id === value);
	let resolved: 'light' | 'dark';

	if (pack) {
		resolved = pack.payload.dark ? 'dark' : 'light';
		document.documentElement.dataset.theme = resolved;
		for (const [key, vars] of Object.entries(THEME_VARS)) {
			const color = pack.payload.colors[key];
			if (!color) continue;
			for (const cssVar of vars) {
				document.documentElement.style.setProperty(cssVar, color);
				appliedVars.push(cssVar);
			}
		}
	} else if (value === 'system') {
		resolved = systemPrefersDark() ? 'dark' : 'light';
		document.documentElement.dataset.theme = resolved;
	} else {
		resolved = value === 'dark' ? 'dark' : 'light';
		document.documentElement.dataset.theme = resolved;
	}

	document.documentElement.style.colorScheme = resolved;
	if (typeof localStorage !== 'undefined') {
		// Cache only the resolved base scheme for the pre-paint script in app.html.
		try {
			localStorage.setItem('chatbox.theme', resolved);
		} catch {
			// The cache is only an optimization; ignore storage failures.
		}
	}
}

/** Activate a theme and persist it for the current user. */
export function setTheme(userId: string | null, value: string): void {
	applyTheme(value);
	if (userId) setActiveTheme(userId, value);
}

/** Restore the user's saved theme, falling back to the built-in light scheme. */
export function syncTheme(userId: string): void {
	const available = new Set([
		...BASE_THEMES.map((option) => option.id),
		...themePacks(get(installedPlugins)).map((pack) => pack.payload.id)
	]);
	const stored = getActiveTheme(userId);
	const resolved = stored && available.has(stored) ? stored : 'light';
	applyTheme(resolved);
	setActiveTheme(userId, resolved);
}

/** Reset the theme after sign-out. */
export function resetTheme(): void {
	applyTheme('light');
}
