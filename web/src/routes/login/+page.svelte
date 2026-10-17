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
	<h1>{tr('app_name')}</h1>
	<form on:submit={submit}>
		<label>
			{tr('user_id')}
			<input bind:value={user_id} autocomplete="username" required />
		</label>
		<label>
			{tr('password')}
			<input type="password" bind:value={password} autocomplete="current-password" required />
		</label>
		{#if error}<p class="error">{error}</p>{/if}
		<button disabled={busy}>{tr('login')}</button>
	</form>
	<p>No account? <a href="/register">{tr('register')}</a></p>
</section>

<style>
	.auth {
		max-width: 22rem;
		margin: 3rem auto;
		padding: 1.5rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 12px;
	}
	form {
		display: flex;
		flex-direction: column;
		gap: 0.8rem;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		color: var(--muted);
	}
	input {
		padding: 0.5rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
	}
	.error {
		color: #dc2626;
		font-size: 0.85rem;
	}
	a {
		color: var(--accent);
	}
</style>
