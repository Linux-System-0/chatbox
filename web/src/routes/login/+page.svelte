<script lang="ts">
	import { goto } from '$app/navigation';
	import { translator } from '$lib/i18n';
	import { postJson } from '$lib/api';
	import { saveSession, type User } from '$lib/session';

	$: tr = $translator;
	let user_id = '';
	let password = '';
	let error = '';
	let busy = false;

	async function submit(event: Event) {
		event.preventDefault();
		error = '';
		busy = true;
		try {
			const response = await postJson<{
				user: User;
				tokens: {
					access_token: string;
					refresh_token: string;
					session_id: string;
					device_id: number;
				};
			}>('/auth/login', { user_id, password, device_name: 'web', platform: 'web' });
			saveSession({
				user: response.user,
				accessToken: response.tokens.access_token,
				refreshToken: response.tokens.refresh_token,
				sessionId: response.tokens.session_id,
				deviceId: response.tokens.device_id
			});
			goto('/chat');
		} catch (e) {
			error = e instanceof Error ? e.message : 'login failed';
		} finally {
			busy = false;
		}
	}
</script>

<section class="auth">
	<div class="auth-card">
		<div class="auth-brand">
			<span class="logo" aria-hidden="true">
				<svg viewBox="0 0 24 24" width="22" height="22" fill="none">
					<path
						d="M4 5.5A2.5 2.5 0 0 1 6.5 3h11A2.5 2.5 0 0 1 20 5.5v8a2.5 2.5 0 0 1-2.5 2.5H9l-4.2 3.36A1 1 0 0 1 3 18.58V5.5Z"
						fill="currentColor"
					/>
				</svg>
			</span>
			<h1>{tr('app_name')}</h1>
			<p class="tagline">{tr('tagline')}</p>
		</div>

		<form on:submit={submit}>
			<label>
				<span>{tr('user_id')}</span>
				<input bind:value={user_id} autocomplete="username" placeholder={tr('user_id')} required />
			</label>
			<label>
				<span>{tr('password')}</span>
				<input
					type="password"
					bind:value={password}
					autocomplete="current-password"
					placeholder="••••••••"
					required
				/>
			</label>
			{#if error}<p class="error" role="alert">{error}</p>{/if}
			<button disabled={busy}>{busy ? '…' : tr('login')}</button>
		</form>

		<p class="switch">{tr('no_account')} <a href="/register">{tr('register')}</a></p>
	</div>
</section>

<style>
	.auth {
		flex: 1;
		display: grid;
		place-items: center;
		padding: 2rem 1rem;
		background:
			radial-gradient(60rem 30rem at 15% -10%, var(--accent-soft), transparent 60%),
			radial-gradient(50rem 30rem at 110% 120%, var(--accent-soft), transparent 55%);
	}
	.auth-card {
		width: 100%;
		max-width: 24rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-md);
		padding: 2rem;
	}
	.auth-brand {
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 0.3rem;
		margin-bottom: 1.5rem;
	}
	.logo {
		display: grid;
		place-items: center;
		width: 3rem;
		height: 3rem;
		border-radius: 14px;
		background: linear-gradient(135deg, var(--accent), #8b5cf6);
		color: #fff;
		box-shadow: var(--shadow-md);
		margin-bottom: 0.5rem;
	}
	h1 {
		font-size: 1.4rem;
		margin: 0;
		letter-spacing: -0.02em;
	}
	.tagline {
		margin: 0;
		font-size: 0.85rem;
		color: var(--muted);
	}
	form {
		display: flex;
		flex-direction: column;
		gap: 0.9rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--muted);
	}
	input {
		padding: 0.65rem 0.75rem;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg);
		color: var(--text);
		font: inherit;
		font-size: 0.95rem;
		transition: border-color 0.15s ease, box-shadow 0.15s ease;
	}
	input:focus {
		outline: none;
		border-color: var(--accent);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
	}
	form button {
		margin-top: 0.3rem;
		padding: 0.7rem;
		font-size: 0.95rem;
	}
	.error {
		margin: 0;
		background: var(--danger-bg);
		color: var(--danger-text);
		border: 1px solid var(--danger-border);
		border-radius: var(--radius-sm);
		padding: 0.5rem 0.65rem;
		font-size: 0.82rem;
	}
	.switch {
		margin: 1.25rem 0 0;
		text-align: center;
		font-size: 0.85rem;
		color: var(--muted);
	}
	a {
		color: var(--accent);
		font-weight: 600;
		text-decoration: none;
	}
	a:hover {
		text-decoration: underline;
	}
</style>
