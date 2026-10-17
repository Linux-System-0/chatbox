<script lang="ts">
	import { get } from 'svelte/store';
	import Avatar from '$lib/Avatar.svelte';
	import { fileToAvatar, effectiveName } from '$lib/avatar';
	import { patchJson } from '$lib/api';
	import { translator } from '$lib/i18n';
	import { migrateUserStorage } from '$lib/plugins';
	import { saveSession, session, type User } from '$lib/session';

	export let onClose: () => void = () => {};

	$: tr = $translator;

	const current = get(session);
	let userId = current?.user.user_id ?? '';
	let displayName = current?.user.display_name ?? '';
	let avatar = current?.user.avatar_url ?? '';
	let busy = false;
	let error = '';

	$: previewName = effectiveName(null, displayName, userId);

	async function chooseAvatar(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		error = '';
		busy = true;
		try {
			avatar = await fileToAvatar(file);
		} catch {
			error = tr('avatar_error');
		} finally {
			busy = false;
			input.value = '';
		}
	}

	async function save(event: Event) {
		event.preventDefault();
		const active = get(session);
		if (!active) return;
		error = '';
		busy = true;
		try {
			const response = await patchJson<{ user: User }>('/users/me', {
				user_id: userId.trim(),
				display_name: displayName.trim() || null,
				avatar_url: avatar || null
			});
			if (active.user.user_id !== response.user.user_id) {
				migrateUserStorage(active.user.user_id, response.user.user_id);
			}
			saveSession({ ...active, user: response.user });
			onClose();
		} catch (e) {
			error = e instanceof Error ? e.message : tr('profile_error');
		} finally {
			busy = false;
		}
	}
</script>

<div class="overlay" role="presentation">
	<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="profile-title">
		<header>
			<h2 id="profile-title">{tr('edit_profile')}</h2>
			<button class="ghost close" on:click={onClose} aria-label={tr('close')} title={tr('close')}>×</button>
		</header>

		<form on:submit={save}>
			<div class="avatar-row">
				<Avatar src={avatar} name={previewName} id={current?.user.id ?? 0} size="lg" />
				<div class="avatar-actions">
					<label class="upload">
						<span>{tr('change_avatar')}</span>
						<input type="file" accept="image/*" on:change={chooseAvatar} disabled={busy} />
					</label>
					{#if avatar}
						<button type="button" class="ghost" on:click={() => (avatar = '')}>{tr('remove_avatar')}</button>
					{/if}
				</div>
			</div>

			<label>
				<span>{tr('user_id')}</span>
				<input bind:value={userId} autocomplete="username" required />
				<small>{tr('user_id_hint')}</small>
			</label>
			<label>
				<span>{tr('display_name')}</span>
				<input bind:value={displayName} autocomplete="nickname" placeholder={tr('display_name')} />
			</label>

			{#if error}<p class="err" role="alert">{error}</p>{/if}

			<div class="actions">
				<button type="button" class="ghost" on:click={onClose}>{tr('cancel')}</button>
				<button type="submit" disabled={busy || !userId.trim()}>
					{busy ? '…' : tr('save')}
				</button>
			</div>
		</form>
	</div>
</div>

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(8, 10, 16, 0.55);
		display: grid;
		place-items: center;
		padding: 1rem;
		z-index: 70;
	}
	.dialog {
		width: min(28rem, 100%);
		max-height: min(85vh, 40rem);
		overflow: auto;
		background: var(--surface);
		color: var(--text);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-md);
		padding: 1.1rem 1.2rem 1.3rem;
	}
	header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
	h2 {
		margin: 0;
		font-size: 1.05rem;
		flex: 1;
	}
	.close {
		padding: 0.25rem 0.55rem;
		font-size: 1.1rem;
		line-height: 1;
	}
	.avatar-row {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin: 1rem 0;
	}
	.avatar-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		align-items: center;
	}
	.upload {
		display: inline-flex;
		align-items: center;
		padding: 0.45rem 0.8rem;
		border-radius: var(--radius-sm);
		background: var(--accent);
		color: var(--accent-contrast);
		font-weight: 600;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.upload input {
		position: absolute;
		width: 1px;
		height: 1px;
		opacity: 0;
		pointer-events: none;
	}
	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		margin-bottom: 0.8rem;
		font-size: 0.85rem;
		font-weight: 600;
	}
	label small {
		color: var(--muted);
		font-weight: 400;
	}
	input {
		padding: 0.5rem 0.65rem;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg);
		color: var(--text);
		font: inherit;
		font-weight: 400;
	}
	input:focus {
		outline: none;
		border-color: var(--accent);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
	}
	.err {
		margin: 0 0 0.7rem;
		padding: 0.5rem 0.65rem;
		border-radius: var(--radius-sm);
		background: var(--danger-bg);
		color: var(--danger-text);
		border: 1px solid var(--danger-border);
		font-size: 0.82rem;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 0.4rem;
	}
</style>
