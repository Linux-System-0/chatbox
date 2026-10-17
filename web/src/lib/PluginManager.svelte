<script lang="ts">
	import { get } from 'svelte/store';
	import { locale, setLocale, syncLocale, translator } from '$lib/i18n';
	import {
		MAX_PLUGIN_BYTES,
		installPlugin,
		installedPlugins,
		isLanguagePlugin,
		isThemePlugin,
		removePlugin,
		type PluginPackage
	} from '$lib/plugins';
	import { setTheme, syncTheme, theme } from '$lib/theme';
	import { session } from '$lib/session';

	export let userId: string;
	export let onClose: () => void = () => {};

	$: tr = $translator;

	let message = '';
	let error = '';
	let busy = false;

	function label(plugin: PluginPackage): string {
		if (isLanguagePlugin(plugin)) return `${plugin.payload.label} (${plugin.payload.locale})`;
		if (isThemePlugin(plugin)) return plugin.payload.label;
		return plugin.name;
	}

	function kindLabel(plugin: PluginPackage): string {
		return tr(plugin.kind === 'language-pack' ? 'kind_language_pack' : 'kind_theme');
	}

	function isActive(plugin: PluginPackage): boolean {
		if (isLanguagePlugin(plugin)) return $locale === plugin.payload.locale;
		if (isThemePlugin(plugin)) return $theme === plugin.payload.id;
		return false;
	}

	async function install(event: Event): Promise<void> {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;

		message = '';
		error = '';
		busy = true;
		try {
			if (file.size > MAX_PLUGIN_BYTES) throw new Error(tr('plugin_file_too_large'));
			const plugin = installPlugin(userId, await file.text());
			// The user may have signed out while the file was being read.
			if (get(session)?.user.user_id !== userId) return;
			// Drop a previously active language/theme if this install replaced it.
			syncLocale(userId);
			syncTheme(userId);
			if (isLanguagePlugin(plugin)) setLocale(userId, plugin.payload.locale);
			if (isThemePlugin(plugin)) setTheme(userId, plugin.payload.id);
			message = `${tr('plugin_installed')}: ${plugin.name}`;
		} catch (e) {
			error = `${tr('plugin_error')}: ${e instanceof Error ? e.message : String(e)}`;
		} finally {
			busy = false;
			input.value = '';
		}
	}

	function remove(plugin: PluginPackage): void {
		try {
			removePlugin(userId, plugin.name);
			syncLocale(userId);
			syncTheme(userId);
			message = `${tr('plugin_removed')}: ${plugin.name}`;
			error = '';
		} catch (e) {
			error = `${tr('plugin_error')}: ${e instanceof Error ? e.message : String(e)}`;
		}
	}
</script>

<div class="overlay" role="presentation">
	<div
		class="dialog"
		role="dialog"
		aria-modal="true"
		aria-labelledby="plugins-title"
	>
		<header>
			<h2 id="plugins-title">{tr('plugins')}</h2>
			<button class="ghost close" on:click={onClose} aria-label={tr('close')} title={tr('close')}>
				×
			</button>
		</header>

		<p class="hint">{tr('plugin_install_hint')}</p>

		<label class="install">
			<span>{busy ? '…' : tr('install_plugin')}</span>
			<input
				type="file"
				accept="application/json,.json"
				on:change={install}
				disabled={busy}
			/>
		</label>

		{#if message}<p class="ok" role="status">{message}</p>{/if}
		{#if error}<p class="err" role="alert">{error}</p>{/if}

		<h3>{tr('installed_plugins')}</h3>
		{#if $installedPlugins.length === 0}
			<p class="empty">{tr('no_plugins')}</p>
		{:else}
			<ul>
				{#each $installedPlugins as plugin (plugin.name)}
					<li>
						<div class="info">
							<strong>{label(plugin)}</strong>
							<span class="meta">
								{kindLabel(plugin)} · v{plugin.version}
								{#if isActive(plugin)}· <span class="active">{tr('active')}</span>{/if}
							</span>
							{#if plugin.description}<p>{plugin.description}</p>{/if}
						</div>
						<button class="ghost" on:click={() => remove(plugin)}>{tr('remove')}</button>
					</li>
				{/each}
			</ul>
		{/if}
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
		z-index: 60;
	}
	.dialog {
		width: min(34rem, 100%);
		max-height: min(80vh, 42rem);
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
	h3 {
		margin: 1.1rem 0 0.5rem;
		font-size: 0.85rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--muted);
	}
	.close {
		padding: 0.25rem 0.55rem;
		font-size: 1.1rem;
		line-height: 1;
	}
	.hint,
	.empty {
		margin: 0.6rem 0;
		color: var(--muted);
		font-size: 0.85rem;
	}
	.install {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		margin: 0.4rem 0 0.2rem;
		padding: 0.5rem 0.85rem;
		border-radius: var(--radius-sm);
		background: var(--accent);
		color: var(--accent-contrast);
		font-weight: 600;
		cursor: pointer;
	}
	.install input {
		position: absolute;
		width: 1px;
		height: 1px;
		opacity: 0;
		pointer-events: none;
	}
	.ok,
	.err {
		margin: 0.7rem 0 0;
		padding: 0.5rem 0.65rem;
		border-radius: var(--radius-sm);
		font-size: 0.85rem;
	}
	.ok {
		background: var(--accent-soft);
		color: var(--text);
	}
	.err {
		background: var(--danger-bg);
		color: var(--danger-text);
		border: 1px solid var(--danger-border);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.55rem;
	}
	li {
		display: flex;
		align-items: flex-start;
		gap: 0.75rem;
		padding: 0.6rem 0.7rem;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--surface-2);
	}
	.info {
		flex: 1;
		min-width: 0;
	}
	.info p {
		margin: 0.25rem 0 0;
		color: var(--muted);
		font-size: 0.8rem;
	}
	.meta {
		display: block;
		margin-top: 0.15rem;
		font-size: 0.78rem;
		color: var(--muted);
	}
	.active {
		color: var(--accent);
		font-weight: 600;
	}
	li button {
		flex: none;
		padding: 0.35rem 0.6rem;
		font-size: 0.78rem;
	}
</style>
