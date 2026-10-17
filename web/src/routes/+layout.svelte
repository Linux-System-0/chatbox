<script lang="ts">
	import { get } from 'svelte/store';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import PluginManager from '$lib/PluginManager.svelte';
	import { locale, languages, resetLocale, setLocale, syncLocale, translator } from '$lib/i18n';
	import { applyTheme, resetTheme, setTheme, syncTheme, theme, themes } from '$lib/theme';
	import { clearPlugins, loadPluginsFor } from '$lib/plugins';
	import { clearSession, loadSession, session } from '$lib/session';

	let showPlugins = false;
	let loadedUserId: string | null = null;

	onMount(() => {
		applyTheme(get(theme));
		loadSession();
		const media = window.matchMedia('(prefers-color-scheme: dark)');
		const onSystemTheme = () => {
			if (get(theme) === 'system') applyTheme('system');
		};
		media.addEventListener('change', onSystemTheme);
		return () => media.removeEventListener('change', onSystemTheme);
	});

	$: tr = $translator;
	$: {
		const userId = $session?.user.user_id ?? null;
		if (userId !== loadedUserId) {
			const wasSignedIn = loadedUserId !== null;
			loadedUserId = userId;
			if (userId) {
				loadPluginsFor(userId);
				syncLocale(userId);
				syncTheme(userId);
			} else if (wasSignedIn) {
				showPlugins = false;
				clearPlugins();
				resetLocale();
				resetTheme();
			}
		}
	}

	function changeLocale(event: Event) {
		const code = (event.currentTarget as HTMLSelectElement).value;
		if ($session) setLocale($session.user.user_id, code);
		else locale.set(code);
	}
	function changeTheme(event: Event) {
		const value = (event.currentTarget as HTMLSelectElement).value;
		if ($session) setTheme($session.user.user_id, value);
		else applyTheme(value);
	}
	function logout() {
		showPlugins = false;
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
				{#each $languages as language}
					<option value={language.code}>{language.label}</option>
				{/each}
			</select>
		</label>
		<label class="control">
			{tr('theme')}
			<select value={$theme} on:change={changeTheme}>
				{#each $themes as option}
					<option value={option.id}>{option.builtin ? tr(option.label) : option.label}</option>
				{/each}
			</select>
		</label>
		{#if $session}
			<button class="ghost" on:click={() => (showPlugins = true)}>{tr('plugins')}</button>
			<button class="ghost" on:click={logout}>{tr('logout')}</button>
		{/if}
	</header>
	<main>
		<slot />
	</main>

	{#if showPlugins && $session}
		<PluginManager userId={$session.user.user_id} onClose={() => (showPlugins = false)} />
	{/if}
</div>

<style>
	:global(:root) {
		--header-h: 3.5rem;
		--bg: #f6f7fb;
		--surface: #ffffff;
		--surface-2: #f1f3f8;
		--surface-3: #e9ecf3;
		--text: #10131a;
		--muted: #667085;
		--border: #e4e7ee;
		--accent: #5b5bd6;
		--accent-hover: #4f4fc4;
		--accent-contrast: #ffffff;
		--accent-soft: #ececfd;
		--bubble-out: #5b5bd6;
		--bubble-out-text: #ffffff;
		--bubble-in: #ffffff;
		--danger-bg: #fef2f2;
		--danger-text: #b42318;
		--danger-border: #fecdca;
		--success: #12b76a;
		--shadow-sm: 0 1px 2px rgba(16, 24, 40, 0.06);
		--shadow-md: 0 8px 24px -6px rgba(16, 24, 40, 0.14);
		--radius-sm: 8px;
		--radius: 12px;
		--radius-lg: 18px;
		color-scheme: light;
	}
	:global([data-theme='dark']) {
		--bg: #0e1016;
		--surface: #161a23;
		--surface-2: #1c212c;
		--surface-3: #232936;
		--text: #e8ebf2;
		--muted: #98a2b3;
		--border: #272d3a;
		--accent: #7c7cf0;
		--accent-hover: #8f8ff5;
		--accent-contrast: #0e1016;
		--accent-soft: #23264a;
		--bubble-out: #5b5bd6;
		--bubble-out-text: #ffffff;
		--bubble-in: #1c212c;
		--danger-bg: #2a1618;
		--danger-text: #fda4a4;
		--danger-border: #5b2429;
		--success: #32d583;
		--shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.4);
		--shadow-md: 0 10px 28px -8px rgba(0, 0, 0, 0.6);
		color-scheme: dark;
	}
	:global(*),
	:global(*::before),
	:global(*::after) {
		box-sizing: border-box;
	}
	:global(body) {
		margin: 0;
		background: var(--bg);
		color: var(--text);
		font-family: 'Inter', system-ui, -apple-system, 'Segoe UI', Roboto, 'Noto Sans SC',
			'PingFang SC', 'Microsoft YaHei', sans-serif;
		font-size: 15px;
		line-height: 1.5;
		-webkit-font-smoothing: antialiased;
		text-rendering: optimizeLegibility;
	}
	:global(:focus-visible) {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}
	:global(*:focus:not(:focus-visible)) {
		outline: none;
	}
	:global(*) {
		scrollbar-width: thin;
		scrollbar-color: var(--border) transparent;
	}
	:global(::-webkit-scrollbar) {
		width: 10px;
		height: 10px;
	}
	:global(::-webkit-scrollbar-thumb) {
		background: var(--border);
		border: 3px solid transparent;
		background-clip: padding-box;
		border-radius: 999px;
	}
	:global(.sr-only) {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
		border: 0;
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
