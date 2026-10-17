<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { deleteJson, getJson, postJson } from '$lib/api';
	import { translator } from '$lib/i18n';
	import { loadSession, session, type User } from '$lib/session';
	import { ChatSocket, type ChatMessage, type ConnectionStatus, type ServerFrame } from '$lib/ws';

	interface ConversationView {
		id: number;
		kind: 'direct' | 'group';
		title: string;
		last_message: ChatMessage | null;
		last_read_seq: number;
		unread: number;
	}
	interface ContactView {
		user: User;
		remark: string | null;
		created_at: string;
	}
	interface BlockView {
		user: User;
		created_at: string;
	}
	interface FriendRequest {
		id: number;
		from_id: number;
		to_id: number;
		status: string;
		message: string | null;
	}

	$: tr = $translator;

	let conversations: ConversationView[] = [];
	let contacts: ContactView[] = [];
	let requests: FriendRequest[] = [];
	let blocks: BlockView[] = [];
	let messages: ChatMessage[] = [];
	let selected: number | null = null;
	let draft = '';
	let searchQuery = '';
	let searchResults: ChatMessage[] = [];
	let newDirect = '';
	let addContact = '';
	let status: ConnectionStatus = 'connecting';
	let error = '';
	let socket: ChatSocket | null = null;
	let messagePane: HTMLDivElement | null = null;

	$: selectedConversation = conversations.find((c) => c.id === selected) ?? null;

	onMount(async () => {
		const current = loadSession();
		if (!current) {
			goto('/login');
			return;
		}
		await refreshAll();
		socket = new ChatSocket(
			current,
			handleFrame,
			(value) => (status = value)
		);
		socket.connect();
	});

	onDestroy(() => socket?.close());

	async function refreshAll() {
		try {
			const [convs, contactList, blockList, requestList] = await Promise.all([
				getJson<{ conversations: ConversationView[] }>('/conversations'),
				getJson<{ contacts: ContactView[] }>('/contacts'),
				getJson<{ blocks: BlockView[] }>('/blocks'),
				getJson<{ requests: FriendRequest[] }>('/contacts/requests')
			]);
			conversations = convs.conversations;
			contacts = contactList.contacts;
			blocks = blockList.blocks;
			requests = requestList.requests;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		}
	}

	async function selectConversation(id: number) {
		selected = id;
		searchResults = [];
		try {
			const response = await getJson<{ messages: ChatMessage[] }>(
				`/conversations/${id}/messages?limit=100`
			);
			messages = response.messages;
			await postJson(`/conversations/${id}/read`, { seq: messages.at(-1)?.seq ?? 0 });
			const conv = conversations.find((c) => c.id === id);
			if (conv) conv.unread = 0;
			await scrollToBottom();
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load messages';
		}
	}

	function handleFrame(frame: ServerFrame) {
		if (frame.type === 'message') {
			const message = frame.message;
			if (message.conversation_id === selected) {
				messages = [...messages, message];
				scrollToBottom();
				if (socket) socket.send({ type: 'ack', conversation_id: selected, seq: message.seq });
			}
			const conv = conversations.find((c) => c.id === message.conversation_id);
			if (conv) {
				conv.last_message = message;
				if (message.conversation_id !== selected) conv.unread += 1;
			}
		} else if (frame.type === 'message_hint') {
			if (frame.conversation_id === selected) {
				selectConversation(frame.conversation_id);
			}
		} else if (frame.type === 'error') {
			error = frame.message;
		}
	}

	function sendMessage() {
		const text = draft.trim();
		if (!text || !selected || !socket) return;
		socket.send({
			type: 'send',
			client_msg_id: crypto.randomUUID(),
			conversation_id: selected,
			kind: 'text',
			content: text
		});
		draft = '';
	}

	async function startDirect() {
		const other = newDirect.trim();
		if (!other) return;
		try {
			const response = await postJson<{ conversation: { id: number } }>('/conversations/direct', {
				user_id: other
			});
			newDirect = '';
			await refreshAll();
			await selectConversation(response.conversation.id);
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to start chat';
		}
	}

	async function runSearch() {
		const q = searchQuery.trim();
		if (!q) {
			searchResults = [];
			return;
		}
		try {
			const response = await getJson<{ results: ChatMessage[] }>(
				`/search?q=${encodeURIComponent(q)}&sort=desc&limit=50`
			);
			searchResults = response.results;
		} catch (e) {
			error = e instanceof Error ? e.message : 'search failed';
		}
	}

	async function jumpTo(result: ChatMessage) {
		searchResults = [];
		await selectConversation(result.conversation_id);
	}

	async function requestContact() {
		const id = addContact.trim();
		if (!id) return;
		try {
			await postJson('/contacts/requests', { user_id: id });
			addContact = '';
			await refreshAll();
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to add contact';
		}
	}

	async function acceptRequest(id: number) {
		await postJson(`/contacts/requests/${id}/accept`);
		await refreshAll();
	}

	async function blockUser(user: string) {
		const id = user.trim();
		if (!id) return;
		await postJson(`/blocks/${id}`);
		await refreshAll();
	}

	async function unblock(user_id: string) {
		await deleteJson(`/blocks/${user_id}`);
		await refreshAll();
	}

	async function scrollToBottom() {
		await new Promise((resolve) => setTimeout(resolve, 0));
		if (messagePane) messagePane.scrollTop = messagePane.scrollHeight;
	}
</script>

<div class="chat">
	<aside class="sidebar">
		<div class="new-direct">
			<input placeholder="user id" bind:value={newDirect} on:keydown={(e) => e.key === 'Enter' && startDirect()} />
			<button on:click={startDirect}>+</button>
		</div>
		<h2>{tr('conversations')}</h2>
		{#if conversations.length === 0}
			<p class="muted">{tr('no_conversations')}</p>
		{/if}
		<ul>
			{#each conversations as conversation}
				<li>
					<button
						class="conversation"
						class:selected={conversation.id === selected}
						on:click={() => selectConversation(conversation.id)}
					>
						<span class="title">{conversation.title}</span>
						<span class="preview">{conversation.last_message?.content ?? ''}</span>
						{#if conversation.unread > 0}<span class="badge">{conversation.unread}</span>{/if}
					</button>
				</li>
			{/each}
		</ul>
		<h2>{tr('contacts')}</h2>
		<div class="new-direct">
			<input placeholder="user id" bind:value={addContact} on:keydown={(e) => e.key === 'Enter' && requestContact()} />
			<button on:click={requestContact}>+</button>
		</div>
		<ul>
			{#each contacts as contact}
				<li class="contact">
					<span
						class="dot"
						class:on={false}
						title={contact.user.user_id}
					></span>
					{contact.remark ?? contact.user.display_name ?? contact.user.user_id}
					<button class="ghost small" on:click={() => blockUser(contact.user.user_id)}>⊘</button>
				</li>
			{/each}
		</ul>
		{#if requests.filter((r) => r.status === 'pending').length > 0}
			<h2>Requests</h2>
			<ul>
				{#each requests.filter((r) => r.status === 'pending') as request}
					<li class="contact">
						#{request.id}
						<button class="ghost small" on:click={() => acceptRequest(request.id)}>✓</button>
					</li>
				{/each}
			</ul>
		{/if}
		{#if blocks.length > 0}
			<h2>{tr('blocks')}</h2>
			<ul>
				{#each blocks as block}
					<li class="contact">
						{block.user.user_id}
						<button class="ghost small" on:click={() => unblock(block.user.user_id)}>×</button>
					</li>
				{/each}
			</ul>
		{/if}
	</aside>

	<section class="main">
		<div class="toolbar">
			<strong>{selectedConversation?.title ?? '—'}</strong>
			<span class="status {tr(status)}">{status}</span>
			<div class="search">
				<input
					placeholder={tr('search_placeholder')}
					bind:value={searchQuery}
					on:keydown={(e) => e.key === 'Enter' && runSearch()}
				/>
				<button on:click={runSearch}>{tr('search')}</button>
			</div>
		</div>

		{#if error}<div class="error">{error}<button class="ghost small" on:click={() => (error = '')}>×</button></div>{/if}

		{#if searchResults.length > 0}
			<div class="search-results">
				{#each searchResults as result}
					<button class="result" on:click={() => jumpTo(result)}>
						<span class="seq">#{result.seq}</span>
						<span>{result.content ?? `[${result.kind}]`}</span>
					</button>
				{/each}
			</div>
		{/if}

		<div class="messages" bind:this={messagePane}>
			{#if selected === null}
				<p class="muted">Select a conversation.</p>
			{/if}
			{#each messages as message}
				<div
					class="bubble"
					class:mine={message.sender_id === $session?.user.id}
				>
					{message.content ?? `[${message.kind}]`}
					<span class="time">{new Date(message.created_at).toLocaleTimeString()}</span>
				</div>
			{/each}
		</div>

		{#if selected !== null}
			<form class="composer" on:submit|preventDefault={sendMessage}>
				<textarea
					rows="1"
					placeholder={tr('message_placeholder')}
					bind:value={draft}
					on:keydown={(e) => {
						if (e.key === 'Enter' && !e.shiftKey) {
							e.preventDefault();
							sendMessage();
						}
					}}
				></textarea>
				<button>{tr('send')}</button>
			</form>
		{/if}
	</section>
</div>

<style>
	.chat {
		flex: 1;
		display: grid;
		grid-template-columns: 18rem 1fr;
		min-height: 0;
		height: calc(100vh - 53px);
	}
	.sidebar {
		border-right: 1px solid var(--border);
		background: var(--surface);
		padding: 0.75rem;
		overflow-y: auto;
	}
	.sidebar h2 {
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--muted);
		margin: 1rem 0 0.4rem;
	}
	.sidebar ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.new-direct {
		display: flex;
		gap: 0.35rem;
	}
	.new-direct input {
		flex: 1;
		min-width: 0;
	}
	.conversation {
		width: 100%;
		text-align: left;
		background: transparent;
		color: var(--text);
		display: grid;
		grid-template-columns: 1fr auto;
		gap: 0.1rem 0.4rem;
		padding: 0.45rem 0.5rem;
		margin-bottom: 0.15rem;
	}
	.conversation.selected {
		background: var(--bubble-out);
	}
	.conversation .title {
		font-weight: 600;
	}
	.conversation .preview {
		grid-column: 1 / 2;
		font-size: 0.78rem;
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.badge {
		grid-row: 1 / 3;
		align-self: center;
		background: var(--accent);
		color: var(--accent-contrast);
		border-radius: 999px;
		font-size: 0.7rem;
		padding: 0.05rem 0.4rem;
	}
	.contact {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.25rem 0.5rem;
		font-size: 0.9rem;
	}
	.dot {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 50%;
		background: var(--border);
	}
	.main {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.toolbar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.55rem 0.9rem;
		border-bottom: 1px solid var(--border);
		background: var(--surface);
	}
	.status {
		font-size: 0.72rem;
		color: var(--muted);
	}
	.search {
		margin-left: auto;
		display: flex;
		gap: 0.35rem;
	}
	.search input {
		padding: 0.3rem 0.5rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
	}
	.error {
		background: #fee2e2;
		color: #991b1b;
		padding: 0.4rem 0.75rem;
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	.search-results {
		max-height: 12rem;
		overflow-y: auto;
		border-bottom: 1px solid var(--border);
		background: var(--surface);
	}
	.result {
		display: block;
		width: 100%;
		text-align: left;
		background: transparent;
		color: var(--text);
		padding: 0.4rem 0.75rem;
	}
	.result .seq {
		color: var(--muted);
		margin-right: 0.5rem;
	}
	.messages {
		flex: 1;
		overflow-y: auto;
		padding: 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.bubble {
		align-self: flex-start;
		background: var(--bubble-in);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 0.45rem 0.65rem;
		max-width: 32rem;
		word-break: break-word;
	}
	.bubble.mine {
		align-self: flex-end;
		background: var(--bubble-out);
	}
	.bubble .time {
		display: block;
		font-size: 0.65rem;
		color: var(--muted);
		margin-top: 0.2rem;
	}
	.composer {
		display: flex;
		gap: 0.5rem;
		padding: 0.6rem 0.9rem;
		border-top: 1px solid var(--border);
		background: var(--surface);
	}
	.composer textarea {
		flex: 1;
		resize: none;
		padding: 0.5rem;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg);
		color: var(--text);
	}
	.muted {
		color: var(--muted);
		font-size: 0.85rem;
	}
	.small {
		padding: 0.1rem 0.4rem;
		font-size: 0.8rem;
	}
	input {
		padding: 0.3rem 0.5rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
	}
</style>
