import { writable } from 'svelte/store';

export interface User {
	id: number;
	user_id: string;
	display_name?: string | null;
	avatar_url?: string | null;
}

export interface Session {
	user: User;
	accessToken: string;
	refreshToken: string;
	sessionId: string;
	deviceId: number;
}

const STORAGE_KEY = 'chatbox.session';

export const session = writable<Session | null>(null);

export function loadSession(): Session | null {
	if (typeof localStorage === 'undefined') return null;
	const raw = localStorage.getItem(STORAGE_KEY);
	if (!raw) return null;
	try {
		const parsed = JSON.parse(raw) as Session;
		session.set(parsed);
		return parsed;
	} catch {
		localStorage.removeItem(STORAGE_KEY);
		return null;
	}
}

export function saveSession(value: Session): void {
	session.set(value);
	if (typeof localStorage !== 'undefined') {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
	}
}

export function clearSession(): void {
	session.set(null);
	if (typeof localStorage !== 'undefined') {
		localStorage.removeItem(STORAGE_KEY);
	}
}
