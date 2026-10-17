import { get } from 'svelte/store';
import { clearSession, saveSession, session, type Session } from './session';

const API_BASE = '/api/v1';

export class ApiError extends Error {
	constructor(
		public status: number,
		public code: string,
		message: string
	) {
		super(message);
	}
}

async function raw(path: string, init: RequestInit): Promise<Response> {
	return fetch(`${API_BASE}${path}`, init);
}

/** Perform an authenticated request, refreshing the token once on 401. */
export async function api<T>(method: string, path: string, body?: unknown): Promise<T> {
	let current = get(session);
	const buildInit = (token?: string): RequestInit => ({
		method,
		headers: {
			'content-type': 'application/json',
			...(token ? { authorization: `Bearer ${token}` } : {})
		},
		body: body === undefined ? undefined : JSON.stringify(body)
	});

	let response = await raw(path, buildInit(current?.accessToken));
	if (response.status === 401 && current?.refreshToken) {
		const refreshed = await raw('/auth/refresh', {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ refresh_token: current.refreshToken })
		});
		if (refreshed.ok) {
			const data = (await refreshed.json()) as {
				tokens: { access_token: string; refresh_token: string; session_id: string; device_id: number };
			};
			const next: Session = {
				...current,
				accessToken: data.tokens.access_token,
				refreshToken: data.tokens.refresh_token,
				sessionId: data.tokens.session_id,
				deviceId: data.tokens.device_id
			};
			saveSession(next);
			current = next;
			response = await raw(path, buildInit(current.accessToken));
		}
	}

	if (!response.ok) {
		let code = 'unknown';
		let message = response.statusText;
		try {
			const payload = (await response.json()) as { error?: { code?: string; message?: string } };
			code = payload.error?.code ?? code;
			message = payload.error?.message ?? message;
		} catch {
			// keep the status text
		}
		if (response.status === 401) {
			clearSession();
		}
		throw new ApiError(response.status, code, message);
	}

	if (response.status === 204) {
		return undefined as T;
	}
	return (await response.json()) as T;
}

export const getJson = <T>(path: string) => api<T>('GET', path);
export const postJson = <T>(path: string, body?: unknown) => api<T>('POST', path, body);
export const patchJson = <T>(path: string, body?: unknown) => api<T>('PATCH', path, body);
export const deleteJson = <T>(path: string) => api<T>('DELETE', path);
