<script lang="ts">
	import { get } from 'svelte/store';
	import { translator } from '$lib/i18n';
	import { session } from '$lib/session';
	import {
		formatTimestamp,
		normalizeTimeSettings,
		resetTimeSettings,
		saveTimeSettings,
		timeSettings,
		timeZoneOptions,
		type TimeSettings,
		type TimeZoneSetting
	} from '$lib/time';

	export let onClose: () => void = () => {};

	$: tr = $translator;

	const active = get(session);
	const initial = get(timeSettings);
	let zone = String(initial.timeZone);
	let mode: 'system' | 'custom' = initial.format ? 'custom' : 'system';
	let customFormat = initial.format;

	$: zoneOptions = timeZoneOptions(new Date(), tr('time_zone_system'));
	$: draft = normalizeTimeSettings({
		timeZone: (zone === 'system' ? 'system' : Number(zone)) as TimeZoneSetting,
		format: mode === 'custom' ? customFormat : ''
	});
	$: preview = formatTimestamp(new Date(), draft);

	function save() {
		saveTimeSettings(active?.user.id ?? null, draft as TimeSettings);
		onClose();
	}

	function reset() {
		resetTimeSettings(active?.user.id ?? null);
		onClose();
	}
</script>

<div class="overlay" role="presentation">
	<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="time-settings-title">
		<header>
			<h2 id="time-settings-title">{tr('time_settings')}</h2>
			<button class="ghost close" on:click={onClose} aria-label={tr('close')} title={tr('close')}
				>×</button
			>
		</header>

		<label>
			<span>{tr('time_zone')}</span>
			<select bind:value={zone}>
				{#each zoneOptions as option (option.value)}
					<option value={option.value}>{option.label}</option>
				{/each}
			</select>
			<small>{tr('time_zone_hint')}</small>
		</label>

		<label>
			<span>{tr('time_format')}</span>
			<select bind:value={mode}>
				<option value="system">{tr('time_format_system')}</option>
				<option value="custom">{tr('time_format_custom')}</option>
			</select>
		</label>

		{#if mode === 'custom'}
			<label>
				<span>{tr('time_pattern')}</span>
				<input bind:value={customFormat} placeholder="YYYY-MM-DD HH:mm:ss" />
				<small>{tr('time_format_hint')}</small>
			</label>
		{/if}

		<div class="preview">
			<span>{tr('time_preview')}</span>
			<strong>{preview}</strong>
		</div>

		<div class="actions">
			<button type="button" class="ghost" on:click={reset}>{tr('reset_defaults')}</button>
			<span class="spacer"></span>
			<button type="button" class="ghost" on:click={onClose}>{tr('cancel')}</button>
			<button type="button" on:click={save}>{tr('save')}</button>
		</div>
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
	label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		margin: 0.9rem 0 0;
		font-size: 0.85rem;
		font-weight: 600;
	}
	label small {
		color: var(--muted);
		font-weight: 400;
	}
	select,
	input {
		padding: 0.5rem 0.65rem;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg);
		color: var(--text);
		font: inherit;
		font-weight: 400;
	}
	select:focus,
	input:focus {
		outline: none;
		border-color: var(--accent);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
	}
	.preview {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		margin-top: 1rem;
		padding: 0.7rem 0.8rem;
		border-radius: var(--radius-sm);
		background: var(--surface-2);
		font-size: 0.85rem;
	}
	.preview span {
		color: var(--muted);
	}
	.actions {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-top: 1.1rem;
	}
	.spacer {
		flex: 1;
	}
</style>
