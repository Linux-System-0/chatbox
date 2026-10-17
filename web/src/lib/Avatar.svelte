<script lang="ts">
	export let src: string | null | undefined = null;
	export let name = '';
	export let id = 0;
	export let size: 'sm' | 'md' | 'lg' = 'md';
	export let muted = false;

	let failed = false;
	let lastSrc: string | null | undefined = null;

	$: if (src !== lastSrc) {
		lastSrc = src;
		failed = false;
	}
	$: showImage = !failed && Boolean(src);
	$: initial = name.trim().slice(0, 1).toUpperCase() || '#';
	$: hue = ((Math.trunc(id) * 47) % 360 + 360) % 360;

	function fail() {
		failed = true;
	}
</script>

<span class="avatar {size}" class:muted style="--hue: {hue}" aria-hidden="true">
	{#if showImage}
		<img {src} alt="" on:error={fail} />
	{:else}
		{initial}
	{/if}
</span>

<style>
	.avatar {
		display: inline-grid;
		place-items: center;
		flex: none;
		width: 2.3rem;
		height: 2.3rem;
		overflow: hidden;
		border-radius: 50%;
		background: hsl(var(--hue, 250) 62% 55%);
		color: #fff;
		font-weight: 700;
		font-size: 0.95rem;
		line-height: 1;
		user-select: none;
	}
	:global([data-theme='dark']) .avatar {
		background: hsl(var(--hue, 250) 45% 38%);
	}
	.avatar.sm {
		width: 1.8rem;
		height: 1.8rem;
		font-size: 0.78rem;
	}
	.avatar.lg {
		width: 3.6rem;
		height: 3.6rem;
		font-size: 1.35rem;
	}
	.avatar.muted {
		background: var(--surface-3);
		color: var(--muted);
	}
	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
</style>
