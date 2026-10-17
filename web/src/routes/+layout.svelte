<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { locale, languages, translator } from '$lib/i18n';
	import { theme, applyTheme, type Theme } from '$lib/theme';
	import { clearSession, loadSession, session } from '$lib/session';

	onMount(() => {
		loadSession();
		applyTheme($theme);
	});

	$: tr = $translator;

	function changeLocale(event: Event) {
		locale.set((event.currentTarget as HTMLSelectElement).value);
	}
	function changeTheme(event: Event) {
		applyTheme((event.currentTarget as HTMLSelectElement).value as Theme);
	}
	function logout() {
		clearSession();
		goto('/login');
	}
</script>

<div class="app">
	<header>
		<a class="brand" href="/chat">ChatBox</a>
		<span class="spacer"></span>
		<label class="control">
			{tr('language')}
			<select value={$locale} on:change={changeLocale}>
				{#each languages as language}
					<option value={language.code}>{language.label}</option>
				{/each}
			</select>
		</label>
		<label class="control">
			{tr('theme')}
			<select value={$theme} on:change={changeTheme}>
				<option value="light">{tr('light')}</option>
				<option value="dark">{tr('dark')}</option>
			</select>
		</label>
		{#if $session}
			<button class="ghost" on:click={logout}>{tr('logout')}</button>
		{/if}
	</header>
	<main>
		<slot />
	</main>
</div>

<style>
	:global(:root) {
		--bg: #f5f6f8;
		--surface: #ffffff;
		--text: #1c1e21;
		--muted: #6b7280;
		--border: #e2e4e9;
		--accent: #4f46e5;
		--accent-contrast: #ffffff;
		--bubble-out: #dbeafe;
		--bubble-in: #ffffff;
	}
	:global([data-theme='dark']) {
		--bg: #12141a;
		--surface: #1b1e26;
		--text: #e5e7eb;
		--muted: #9ca3af;
		--border: #2b2f3a;
		--accent: #818cf8;
		--accent-contrast: #10121a;
		--bubble-out: #1e3a5f;
		--bubble-in: #232733;
	}
	:global(body) {
		margin: 0;
		background: var(--bg);
		color: var(--text);
		font-family: system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
	}
	.app {
		min-height: 100vh;
		display: flex;
		flex-direction: column;
	}
	header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.6rem 1rem;
		background: var(--surface);
		border-bottom: 1px solid var(--border);
	}
	.brand {
		font-weight: 700;
		color: var(--accent);
		text-decoration: none;
		font-size: 1.1rem;
	}
	.spacer {
		flex: 1;
	}
	.control {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.8rem;
		color: var(--muted);
	}
	select,
	button {
		font: inherit;
	}
	select {
		background: var(--bg);
		color: var(--text);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 0.2rem 0.4rem;
	}
	button {
		border: none;
		border-radius: 6px;
		padding: 0.45rem 0.8rem;
		background: var(--accent);
		color: var(--accent-contrast);
		cursor: pointer;
	}
	button.ghost {
		background: transparent;
		color: var(--text);
		border: 1px solid var(--border);
	}
	main {
		flex: 1;
		display: flex;
		flex-direction: column;
	}
</style>
