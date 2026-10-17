<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { get } from 'svelte/store';
	import { loadSession, session } from '$lib/session';

	onMount(() => {
		loadSession();
		goto(get(session) ? '/chat' : '/login');
	});
</script>
