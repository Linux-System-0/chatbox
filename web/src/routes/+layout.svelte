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
	$: profileName = $session?.user.display_name || $session?.user.user_id || '';
	$: initial = profileName.trim().slice(0, 1).toUpperCase() || '?';
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
		<a class="brand" href="/chat" aria-label="ChatBox">
			<span class="logo" aria-hidden="true">
				<svg viewBox="0 0 24 24" width="18" height="18" fill="none">
					<path
						d="M4 5.5A2.5 2.5 0 0 1 6.5 3h11A2.5 2.5 0 0 1 20 5.5v8a2.5 2.5 0 0 1-2.5 2.5H9l-4.2 3.36A1 1 0 0 1 3 18.58V5.5Z"
						fill="currentColor"
					/>
				</svg>
			</span>
			<span class="brand-name">ChatBox</span>
		</a>

		<span class="spacer"></span>

		<div class="controls">
			<label class="control">
				<span class="sr-only">{tr('language')}</span>
				<select value={$locale} on:change={changeLocale} title={tr('language')}>
					{#each $languages as language}
						<option value={language.code}>{language.label}</option>
					{/each}
				</select>
			</label>
			<label class="control">
				<span class="sr-only">{tr('theme')}</span>
				<select value={$theme} on:change={changeTheme} title={tr('theme')}>
					{#each $themes as option}
						<option value={option.id}>{option.builtin ? tr(option.label) : option.label}</option>
					{/each}
				</select>
			</label>
		</div>

		{#if $session}
			<button
				class="ghost plugin-button"
				on:click={() => (showPlugins = true)}
				aria-label={tr('plugins')}
				title={tr('plugins')}
			>
				🧩 <span class="plugin-label">{tr('plugins')}</span>
			</button>
			<div class="user" title={profileName}>
				<span class="avatar" aria-hidden="true">{initial}</span>
				<span class="username">{profileName}</span>
			</div>
			<button class="ghost logout" on:click={logout} aria-label={tr('logout')} title={tr('logout')}>
				<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
					<path d="M15 4h3a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3" stroke-linecap="round" />
					<path d="M10 8l-4 4 4 4M6 12h9" stroke-linecap="round" stroke-linejoin="round" />
				</svg>
				<span class="logout-label">{tr('logout')}</span>
			</button>
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
		height: var(--header-h);
		padding: 0 1rem;
		background: var(--surface);
		border-bottom: 1px solid var(--border);
		position: sticky;
		top: 0;
		z-index: 20;
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 0.55rem;
		color: var(--text);
		text-decoration: none;
		font-weight: 700;
		letter-spacing: -0.01em;
	}
	.logo {
		display: grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border-radius: 10px;
		background: linear-gradient(135deg, var(--accent), #8b5cf6);
		color: #fff;
		box-shadow: var(--shadow-sm);
	}
	.brand-name {
		font-size: 1.05rem;
	}
	.spacer {
		flex: 1;
	}
	.controls {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}
	.control select {
		font: inherit;
		font-size: 0.82rem;
		background: var(--surface-2);
		color: var(--text);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 0.35rem 0.5rem;
		cursor: pointer;
	}
	.control select:hover {
		background: var(--surface-3);
	}
	.user {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.25rem 0.6rem 0.25rem 0.25rem;
		border-radius: 999px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		max-width: 14rem;
	}
	.logout {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		white-space: nowrap;
	}
	.plugin-button {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		white-space: nowrap;
	}
	.avatar {
		display: grid;
		place-items: center;
		width: 1.7rem;
		height: 1.7rem;
		border-radius: 50%;
		background: linear-gradient(135deg, var(--accent), #8b5cf6);
		color: #fff;
		font-size: 0.8rem;
		font-weight: 700;
		flex: none;
	}
	.username {
		font-size: 0.85rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	:global(button) {
		font: inherit;
		border: none;
		border-radius: var(--radius-sm);
		padding: 0.5rem 0.85rem;
		background: var(--accent);
		color: var(--accent-contrast);
		font-weight: 600;
		cursor: pointer;
		transition:
			background 0.15s ease,
			transform 0.05s ease,
			opacity 0.15s ease;
	}
	:global(button:hover:not(:disabled)) {
		background: var(--accent-hover);
	}
	:global(button:active:not(:disabled)) {
		transform: translateY(1px);
	}
	:global(button:disabled) {
		opacity: 0.55;
		cursor: not-allowed;
	}
	:global(button.ghost) {
		background: transparent;
		color: var(--text);
		border: 1px solid var(--border);
	}
	:global(button.ghost:hover:not(:disabled)) {
		background: var(--surface-2);
	}
	main {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	@media (max-width: 640px) {
		header {
			gap: 0.4rem;
			padding: 0 0.6rem;
		}
		.brand-name,
		.username,
		.user {
			display: none;
		}
		.plugin-label {
			display: none;
		}
		.controls {
			gap: 0.3rem;
		}
		.control select {
			font-size: 0.78rem;
			padding: 0.3rem 0.35rem;
		}
		.logout-label {
			display: none;
		}
		.logout {
			padding: 0.45rem;
		}
	}
</style>
