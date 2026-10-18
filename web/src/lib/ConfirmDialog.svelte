<script lang="ts">
	import { onMount } from 'svelte';
	import { translator } from '$lib/i18n';

	export let title = '';
	export let message: string;
	export let confirmText = '';
	export let cancelText = '';
	export let danger = false;
	export let onConfirm: () => void = () => {};
	export let onCancel: () => void = () => {};

	$: tr = $translator;

	let panel: HTMLDivElement;
	let confirmButton: HTMLButtonElement;

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onCancel();
			return;
		}
		if (event.key !== 'Tab' || !panel) return;
		const focusable = panel.querySelectorAll<HTMLElement>('button:not(:disabled)');
		if (focusable.length === 0) return;
		const first = focusable[0];
		const last = focusable[focusable.length - 1];
		if (event.shiftKey && document.activeElement === first) {
			event.preventDefault();
			last.focus();
		} else if (!event.shiftKey && document.activeElement === last) {
			event.preventDefault();
			first.focus();
		}
	}

	onMount(() => {
		confirmButton?.focus();
		window.addEventListener('keydown', onKeydown);
		return () => window.removeEventListener('keydown', onKeydown);
	});
</script>

<div class="overlay" role="presentation">
	<div
		class="dialog"
		role="alertdialog"
		aria-modal="true"
		aria-labelledby="confirm-dialog-title"
		aria-describedby="confirm-dialog-message"
		bind:this={panel}
	>
		<h2 id="confirm-dialog-title">{title || tr('confirm_title')}</h2>
		<p id="confirm-dialog-message">{message}</p>
		<div class="actions">
			<button type="button" class="ghost" on:click={onCancel}>
				{cancelText || tr('cancel')}
			</button>
			<button
				type="button"
				class:danger
				on:click={onConfirm}
				bind:this={confirmButton}
			>
				{confirmText || tr('confirm')}
			</button>
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
		z-index: 80;
	}
	.dialog {
		width: min(24rem, 100%);
		background: var(--surface);
		color: var(--text);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-md);
		padding: 1.1rem 1.2rem 1.3rem;
	}
	h2 {
		margin: 0 0 0.6rem;
		font-size: 1.02rem;
	}
	p {
		margin: 0;
		color: var(--muted);
		font-size: 0.88rem;
		line-height: 1.45;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 1rem;
	}
	.danger {
		background: var(--danger-text);
		color: #fff;
	}
	.danger:hover:not(:disabled) {
		background: color-mix(in srgb, var(--danger-text) 85%, #000);
	}
</style>
