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
	interface MessageItem {
		message: ChatMessage;
		mine: boolean;
		head: boolean;
		tail: boolean;
		day: string | null;
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
	let sidebarTab: 'chats' | 'contacts' = 'chats';
	let mobileList = true;
	let socket: ChatSocket | null = null;
	let messagePane: HTMLDivElement | null = null;
	let composerEl: HTMLTextAreaElement | null = null;

	$: selectedConversation = conversations.find((c) => c.id === selected) ?? null;
	$: pendingRequests = requests.filter((r) => r.status === 'pending');
	$: messageItems = buildMessageItems(messages, $session?.user.id, tr);

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

	function buildMessageItems(
		list: ChatMessage[],
		selfId: number | undefined,
		t: (key: string) => string
	): MessageItem[] {
		const items: MessageItem[] = [];
		for (let i = 0; i < list.length; i += 1) {
			const message = list[i];
			const previous = list[i - 1];
			const next = list[i + 1];
			const groupable = message.kind !== 'system';
			const joinsPrevious =
				groupable &&
				previous?.kind !== 'system' &&
				previous?.sender_id === message.sender_id &&
				closeInTime(previous.created_at, message.created_at);
			const joinsNext =
				groupable &&
				next?.kind !== 'system' &&
				next?.sender_id === message.sender_id &&
				closeInTime(message.created_at, next.created_at);
			const dayChanged = !previous || dayKey(previous.created_at) !== dayKey(message.created_at);
			items.push({
				message,
				mine: message.sender_id === selfId,
				head: !joinsPrevious,
				tail: !joinsNext,
				day: dayChanged && message.kind !== 'system' ? formatDay(message.created_at, t) : null
			});
		}
		return items;
	}

	function closeInTime(a: string, b: string): boolean {
		return Math.abs(new Date(b).getTime() - new Date(a).getTime()) < 5 * 60 * 1000;
	}

	function dayKey(value: string | Date): string {
		const date = value instanceof Date ? value : new Date(value);
		return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
	}

	function formatDay(value: string, t: (key: string) => string): string {
		const date = new Date(value);
		if (dayKey(date) === dayKey(new Date())) return t('today');
		const yesterday = new Date();
		yesterday.setDate(yesterday.getDate() - 1);
		if (dayKey(date) === dayKey(yesterday)) return t('yesterday');
		return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
	}

	function shortTime(value: string): string {
		return new Date(value).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
	}

	function initialOf(value: string): string {
		return value.trim().slice(0, 1).toUpperCase() || '#';
	}

	function hueOf(id: number): number {
		return (id * 47) % 360;
	}

	function previewOf(conversation: ConversationView): string {
		const message = conversation.last_message;
		return message ? contentOf(message) : '';
	}

	function contentOf(message: ChatMessage): string {
		return message.content ?? (message.kind === 'file' ? tr('file') : `[${message.kind}]`);
	}

	function autoGrow(node: HTMLTextAreaElement) {
		const resize = () => {
			node.style.height = 'auto';
			const scroll = node.scrollHeight;
			node.style.height = `${Math.min(scroll + 2, 160)}px`;
			node.style.overflowY = scroll + 2 > 160 ? 'auto' : 'hidden';
		};
		resize();
		node.addEventListener('input', resize);
		return {
			destroy() {
				node.removeEventListener('input', resize);
			}
		};
	}

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
		mobileList = false;
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
		if (composerEl) composerEl.style.height = 'auto';
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

<div class="chat" class:mobile-list={mobileList}>
	<aside class="sidebar">
		<div class="side-head">
			<div class="tabs" role="tablist">
				<button
					class="tab"
					class:active={sidebarTab === 'chats'}
					role="tab"
					aria-selected={sidebarTab === 'chats'}
					on:click={() => (sidebarTab = 'chats')}
				>
					{tr('chats')}
					{#if conversations.reduce((sum, c) => sum + c.unread, 0) > 0}
						<span class="tab-dot" aria-hidden="true"></span>
					{/if}
				</button>
				<button
					class="tab"
					class:active={sidebarTab === 'contacts'}
					role="tab"
					aria-selected={sidebarTab === 'contacts'}
					on:click={() => (sidebarTab = 'contacts')}
				>
					{tr('contacts')}
				</button>
			</div>
		</div>

		<div class="side-body">
			{#if sidebarTab === 'chats'}
				<form class="new-row" on:submit|preventDefault={startDirect}>
					<input
						placeholder={tr('user_id')}
						aria-label={tr('new_conversation')}
						bind:value={newDirect}
					/>
					<button type="submit" title={tr('new_conversation')} aria-label={tr('new_conversation')}
						>+</button
					>
				</form>
				{#if conversations.length === 0}
					<p class="empty">{tr('no_conversations')}</p>
				{:else}
					<ul class="list">
						{#each conversations as conversation (conversation.id)}
							<li>
								<button
									class="row"
									class:selected={conversation.id === selected}
									on:click={() => selectConversation(conversation.id)}
								>
									<span
										class="row-avatar"
										style="--hue: {hueOf(conversation.id)}"
										aria-hidden="true">{initialOf(conversation.title)}</span
									>
									<span class="row-main">
										<span class="row-top">
											<span class="row-title">{conversation.title}</span>
											{#if conversation.last_message}
												<span class="row-time">{shortTime(conversation.last_message.created_at)}</span>
											{/if}
										</span>
										<span class="row-bottom">
											<span class="row-preview">{previewOf(conversation)}</span>
											{#if conversation.unread > 0}
												<span class="badge">{conversation.unread > 99 ? '99+' : conversation.unread}</span>
											{/if}
										</span>
									</span>
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			{:else}
				<form class="new-row" on:submit|preventDefault={requestContact}>
					<input
						placeholder={tr('user_id')}
						aria-label={tr('add_contact')}
						bind:value={addContact}
					/>
					<button type="submit" title={tr('add_contact')} aria-label={tr('add_contact')}>+</button>
				</form>

				{#if pendingRequests.length > 0}
					<h2>{tr('requests')}</h2>
					<ul class="list">
						{#each pendingRequests as request (request.id)}
							<li class="contact-row">
								<span class="row-avatar small-avatar" style="--hue: {hueOf(request.from_id)}" aria-hidden="true"
									>{initialOf(String(request.from_id))}</span
								>
								<span class="contact-name">#{request.from_id}</span>
								<button class="chip" on:click={() => acceptRequest(request.id)}>{tr('accept')}</button>
							</li>
						{/each}
					</ul>
				{/if}

				<h2>{tr('contacts')}</h2>
				{#if contacts.length === 0}
					<p class="empty">{tr('no_contacts')}</p>
				{:else}
					<ul class="list">
						{#each contacts as contact (contact.user.id)}
							<li class="contact-row">
								<span
									class="row-avatar small-avatar"
									style="--hue: {hueOf(contact.user.id)}"
									aria-hidden="true">{initialOf(contact.user.user_id)}</span
								>
								<span class="contact-name">{contact.remark ?? contact.user.display_name ?? contact.user.user_id}</span>
								<button
									class="chip subtle"
									title={tr('block')}
									on:click={() => blockUser(contact.user.user_id)}>{tr('block')}</button
								>
							</li>
						{/each}
					</ul>
				{/if}

				{#if blocks.length > 0}
					<h2>{tr('blocks')}</h2>
					<ul class="list">
						{#each blocks as block (block.user.id)}
							<li class="contact-row">
								<span class="row-avatar small-avatar muted-avatar" aria-hidden="true"
									>{initialOf(block.user.user_id)}</span
								>
								<span class="contact-name">{block.user.user_id}</span>
								<button class="chip subtle" on:click={() => unblock(block.user.user_id)}
									>{tr('unblock')}</button
								>
							</li>
						{/each}
					</ul>
				{/if}
			{/if}
		</div>
	</aside>

	<section class="main">
		<div class="toolbar">
			<button class="back-btn" on:click={() => (mobileList = true)} aria-label={tr('back')}>
				<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2.2">
					<path d="M15 5l-7 7 7 7" stroke-linecap="round" stroke-linejoin="round" />
				</svg>
			</button>
			{#if selectedConversation}
				<span
					class="toolbar-avatar"
					style="--hue: {hueOf(selectedConversation.id)}"
					aria-hidden="true">{initialOf(selectedConversation.title)}</span
				>
				<div class="toolbar-title">
					<strong>{selectedConversation.title}</strong>
					<span class="status-pill {status}">
						<i class="status-dot"></i>
						{tr(status)}
					</span>
				</div>
			{:else}
				<strong class="toolbar-placeholder">ChatBox</strong>
			{/if}
			<div class="search">
				<input
					placeholder={tr('search_placeholder')}
					aria-label={tr('search')}
					bind:value={searchQuery}
					on:keydown={(e) => e.key === 'Enter' && runSearch()}
				/>
				<button on:click={runSearch}>{tr('search')}</button>
			</div>
		</div>

		{#if error}
			<div class="error" role="alert">
				<span>{error}</span>
				<button class="error-close" on:click={() => (error = '')} aria-label={tr('dismiss')}>×</button>
			</div>
		{/if}

		{#if searchResults.length > 0}
			<div class="search-results">
				<div class="search-results-head">
					<span>{searchResults.length} {searchResults.length === 1 ? tr('result') : tr('results')}</span>
					<button class="error-close" on:click={() => (searchResults = [])} aria-label={tr('close')}>×</button>
				</div>
				{#each searchResults as result (result.id)}
					<button class="result" on:click={() => jumpTo(result)}>
						<span class="result-content">{contentOf(result)}</span>
						<span class="result-meta">
							#{result.seq} · {shortTime(result.created_at)}
						</span>
					</button>
				{/each}
			</div>
		{/if}

		<div class="messages" bind:this={messagePane}>
			{#if selected === null}
				<div class="blank">
					<svg viewBox="0 0 64 64" width="72" height="72" fill="none" aria-hidden="true">
						<rect x="8" y="12" width="48" height="34" rx="8" stroke="currentColor" stroke-width="3" />
						<path d="M20 46l-4 10 12-10" fill="currentColor" opacity="0.15" />
						<circle cx="24" cy="29" r="3" fill="currentColor" />
						<circle cx="32" cy="29" r="3" fill="currentColor" />
						<circle cx="40" cy="29" r="3" fill="currentColor" />
					</svg>
					<p>{tr('select_conversation')}</p>
				</div>
			{:else if messages.length === 0}
				<div class="blank">
					<p>{tr('no_messages')}</p>
				</div>
			{/if}

			{#each messageItems as item (item.message.id)}
				{#if item.day}
					<div class="day"><span>{item.day}</span></div>
				{/if}
				{#if item.message.kind === 'system'}
					<div class="system">{item.message.content}</div>
				{:else}
					<div class="line" class:mine={item.mine} class:head={item.head} class:tail={item.tail}>
						{#if !item.mine}
							<span
								class="msg-avatar"
								class:invisible={!item.head}
								style="--hue: {hueOf(item.message.sender_id)}"
								aria-hidden="true">
								<svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
									<circle cx="12" cy="8" r="4" />
									<path d="M4 20a8 8 0 0 1 16 0z" />
								</svg>
							</span>
						{/if}
						<div class="bubble">
							<span class="content">{contentOf(item.message)}</span>
							{#if item.tail}
								<span class="time">{shortTime(item.message.created_at)}</span>
							{/if}
						</div>
					</div>
				{/if}
			{/each}
		</div>

		{#if selected !== null}
			<form class="composer" on:submit|preventDefault={sendMessage}>
				<textarea
					rows="1"
					placeholder={tr('message_placeholder')}
					aria-label={tr('message_placeholder')}
					bind:value={draft}
					bind:this={composerEl}
					use:autoGrow
					on:keydown={(e) => {
						if (e.key === 'Enter' && !e.shiftKey) {
							e.preventDefault();
							sendMessage();
						}
					}}
				></textarea>
				<button type="submit" disabled={!draft.trim()}>
					<svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
						<path d="M3.4 20.4 21 12 3.4 3.6 3 10l12 2-12 2z" />
					</svg>
					<span class="send-label">{tr('send')}</span>
				</button>
			</form>
		{/if}
	</section>
</div>

<style>
	.chat {
		display: grid;
		grid-template-columns: 20rem 1fr;
		min-height: 0;
		height: calc(100vh - var(--header-h));
		height: calc(100dvh - var(--header-h));
	}
	.sidebar {
		display: flex;
		flex-direction: column;
		min-height: 0;
		border-right: 1px solid var(--border);
		background: var(--surface);
	}
	.side-head {
		padding: 0.6rem 0.75rem 0;
	}
	.tabs {
		display: flex;
		gap: 0.25rem;
		background: var(--surface-2);
		padding: 0.25rem;
		border-radius: var(--radius);
	}
	.tab {
		position: relative;
		flex: 1;
		background: transparent;
		color: var(--muted);
		padding: 0.4rem 0.5rem;
		font-size: 0.85rem;
		border-radius: var(--radius-sm);
	}
	.tab:hover:not(.active) {
		background: transparent;
		color: var(--text);
	}
	.tab.active {
		background: var(--surface);
		color: var(--text);
		box-shadow: var(--shadow-sm);
	}
	.tab-dot {
		display: inline-block;
		width: 6px;
		height: 6px;
		margin-left: 0.35rem;
		border-radius: 50%;
		background: var(--accent);
		vertical-align: middle;
	}
	.side-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 0.75rem;
	}
	.side-body h2 {
		font-size: 0.72rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--muted);
		margin: 1.1rem 0 0.4rem;
		font-weight: 600;
	}
	.side-body h2:first-child {
		margin-top: 0;
	}
	.new-row {
		display: flex;
		gap: 0.4rem;
	}
	.new-row input {
		flex: 1;
		min-width: 0;
	}
	.new-row button {
		width: 2.3rem;
		font-size: 1.1rem;
		line-height: 1;
		padding: 0;
	}
	.list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.row {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 0.65rem;
		text-align: left;
		background: transparent;
		color: var(--text);
		padding: 0.55rem 0.6rem;
		border-radius: var(--radius);
		font-weight: 400;
	}
	.row:hover:not(.selected) {
		background: var(--surface-2);
	}
	.row.selected {
		background: var(--accent-soft);
	}
	.row.selected .row-title {
		color: var(--accent);
	}
	.row-avatar {
		display: grid;
		place-items: center;
		width: 2.5rem;
		height: 2.5rem;
		flex: none;
		border-radius: 50%;
		background: hsl(var(--hue, 250) 62% 55%);
		color: #fff;
		font-weight: 700;
		font-size: 0.95rem;
	}
	:global([data-theme='dark']) .row-avatar {
		background: hsl(var(--hue, 250) 45% 38%);
	}
	.small-avatar {
		width: 2rem;
		height: 2rem;
		font-size: 0.8rem;
	}
	.muted-avatar {
		background: var(--surface-3);
		color: var(--muted);
	}
	.row-main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
	}
	.row-top {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}
	.row-title {
		font-weight: 600;
		font-size: 0.9rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.row-time {
		margin-left: auto;
		font-size: 0.7rem;
		color: var(--muted);
		flex: none;
	}
	.row-bottom {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.row-preview {
		flex: 1;
		min-width: 0;
		font-size: 0.82rem;
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.badge {
		flex: none;
		background: var(--accent);
		color: var(--accent-contrast);
		border-radius: 999px;
		font-size: 0.7rem;
		font-weight: 700;
		min-width: 1.25rem;
		text-align: center;
		padding: 0.05rem 0.4rem;
	}
	.contact-row {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.4rem 0.5rem;
		border-radius: var(--radius);
	}
	.contact-row:hover {
		background: var(--surface-2);
	}
	.contact-name {
		flex: 1;
		min-width: 0;
		font-size: 0.9rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.chip {
		flex: none;
		font-size: 0.75rem;
		font-weight: 600;
		padding: 0.25rem 0.6rem;
		border-radius: 999px;
	}
	.chip.subtle {
		background: transparent;
		color: var(--muted);
		border: 1px solid var(--border);
	}
	.chip.subtle:hover:not(:disabled) {
		background: var(--surface-3);
		color: var(--text);
	}
	.empty {
		color: var(--muted);
		font-size: 0.85rem;
		padding: 0.75rem 0.5rem;
		text-align: center;
	}

	.main {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		background: var(--bg);
	}
	.toolbar {
		display: flex;
		align-items: center;
		gap: 0.65rem;
		padding: 0.55rem 0.9rem;
		border-bottom: 1px solid var(--border);
		background: var(--surface);
		min-height: 3.6rem;
	}
	.toolbar-avatar {
		display: grid;
		place-items: center;
		width: 2.1rem;
		height: 2.1rem;
		flex: none;
		border-radius: 50%;
		background: hsl(var(--hue, 250) 62% 55%);
		color: #fff;
		font-weight: 700;
		font-size: 0.85rem;
	}
	:global([data-theme='dark']) .toolbar-avatar {
		background: hsl(var(--hue, 250) 45% 38%);
	}
	.toolbar-title {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		min-width: 0;
	}
	.toolbar-title strong {
		font-size: 0.95rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.toolbar-placeholder {
		color: var(--muted);
	}
	.status-pill {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.72rem;
		color: var(--muted);
	}
	.status-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--muted);
	}
	.status-pill.online .status-dot {
		background: var(--success);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--success) 20%, transparent);
	}
	.status-pill.connecting .status-dot {
		background: #f59e0b;
		animation: pulse 1.4s ease-in-out infinite;
	}
	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.35;
		}
	}
	.search {
		margin-left: auto;
		display: flex;
		gap: 0.4rem;
	}
	.search input {
		width: 13rem;
		max-width: 34vw;
	}
	.error {
		background: var(--danger-bg);
		color: var(--danger-text);
		border-bottom: 1px solid var(--danger-border);
		padding: 0.5rem 0.9rem;
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.85rem;
	}
	.error-close {
		background: transparent;
		color: inherit;
		padding: 0 0.3rem;
		font-size: 1.1rem;
		line-height: 1;
		flex: none;
	}
	.error-close:hover:not(:disabled) {
		background: transparent;
		opacity: 0.7;
	}
	.search-results {
		max-height: 40%;
		overflow-y: auto;
		border-bottom: 1px solid var(--border);
		background: var(--surface);
	}
	.search-results-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.4rem 0.9rem;
		font-size: 0.75rem;
		color: var(--muted);
		position: sticky;
		top: 0;
		background: var(--surface);
	}
	.result {
		display: flex;
		justify-content: space-between;
		gap: 1rem;
		width: 100%;
		text-align: left;
		background: transparent;
		color: var(--text);
		padding: 0.5rem 0.9rem;
		font-weight: 400;
		border-radius: 0;
	}
	.result:hover:not(:disabled) {
		background: var(--surface-2);
	}
	.result-content {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.result-meta {
		flex: none;
		color: var(--muted);
		font-size: 0.75rem;
	}
	.messages {
		flex: 1;
		overflow-y: auto;
		padding: 1.25rem 1.25rem 1.5rem;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-height: 0;
	}
	.blank {
		margin: auto;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		color: var(--muted);
		text-align: center;
		padding: 2rem;
	}
	.blank svg {
		color: var(--accent);
		opacity: 0.55;
	}
	.blank p {
		margin: 0;
		font-size: 0.9rem;
	}
	.day {
		display: flex;
		justify-content: center;
		margin: 1rem 0 0.6rem;
	}
	.day span {
		background: var(--surface-3);
		color: var(--muted);
		font-size: 0.7rem;
		font-weight: 600;
		padding: 0.15rem 0.7rem;
		border-radius: 999px;
	}
	.system {
		align-self: center;
		background: var(--surface-3);
		color: var(--muted);
		font-size: 0.78rem;
		padding: 0.2rem 0.7rem;
		border-radius: 999px;
		margin: 0.35rem 0;
	}
	.line {
		display: flex;
		align-items: flex-end;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
	.line.head {
		margin-top: 0.75rem;
	}
	.line.mine {
		flex-direction: row-reverse;
	}
	.msg-avatar {
		display: grid;
		place-items: center;
		width: 1.7rem;
		height: 1.7rem;
		flex: none;
		border-radius: 50%;
		background: hsl(var(--hue, 250) 62% 55%);
		color: #fff;
		margin-bottom: 0.15rem;
	}
	:global([data-theme='dark']) .msg-avatar {
		background: hsl(var(--hue, 250) 45% 40%);
	}
	.msg-avatar.invisible {
		visibility: hidden;
	}
	.bubble {
		max-width: min(34rem, 72%);
		background: var(--bubble-in);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 0.5rem 0.75rem;
		word-break: break-word;
		box-shadow: var(--shadow-sm);
	}
	.line:not(.head) .bubble {
		border-top-left-radius: var(--radius-sm);
	}
	.line:not(.tail) .bubble {
		border-bottom-left-radius: var(--radius-sm);
	}
	.line.mine .bubble {
		background: var(--bubble-out);
		color: var(--bubble-out-text);
		border-color: transparent;
	}
	.line.mine:not(.head) .bubble {
		border-top-left-radius: var(--radius-lg);
		border-top-right-radius: var(--radius-sm);
	}
	.line.mine:not(.tail) .bubble {
		border-bottom-left-radius: var(--radius-lg);
		border-bottom-right-radius: var(--radius-sm);
	}
	.content {
		white-space: pre-wrap;
	}
	.time {
		display: block;
		font-size: 0.65rem;
		color: var(--muted);
		margin-top: 0.2rem;
		text-align: right;
	}
	.line.mine .time {
		color: color-mix(in srgb, var(--bubble-out-text) 75%, transparent);
	}
	.composer {
		display: flex;
		align-items: flex-end;
		gap: 0.5rem;
		padding: 0.7rem 0.9rem;
		border-top: 1px solid var(--border);
		background: var(--surface);
	}
	.composer textarea {
		flex: 1;
		resize: none;
		overflow-y: hidden;
		max-height: 10rem;
		padding: 0.6rem 0.75rem;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg);
		color: var(--text);
		font: inherit;
		line-height: 1.4;
		transition: border-color 0.15s ease, box-shadow 0.15s ease;
	}
	.composer textarea:focus {
		outline: none;
		border-color: var(--accent);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
	}
	.composer button {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		height: 2.6rem;
		padding: 0 1rem;
	}
	input {
		padding: 0.5rem 0.65rem;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--surface);
		color: var(--text);
		font: inherit;
		font-size: 0.9rem;
	}
	input::placeholder,
	textarea::placeholder {
		color: var(--muted);
	}
	input:focus,
	textarea:focus {
		outline: none;
		border-color: var(--accent);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
	}

	.back-btn {
		display: none;
	}

	@media (max-width: 760px) {
		.chat {
			grid-template-columns: 1fr;
		}
		.sidebar,
		.main {
			grid-column: 1;
			grid-row: 1;
		}
		.sidebar {
			border-right: none;
			display: none;
		}
		.main {
			display: none;
		}
		.chat.mobile-list .sidebar {
			display: flex;
		}
		.chat:not(.mobile-list) .main {
			display: flex;
		}
		.toolbar {
			flex-wrap: wrap;
			gap: 0.5rem;
			padding: 0.55rem 0.7rem;
		}
		.toolbar-title {
			flex: 1;
		}
		.toolbar-avatar {
			display: none;
		}
		.search {
			flex: 1 0 100%;
			margin-left: 0;
		}
		.search input {
			flex: 1;
			width: auto;
			max-width: none;
		}
		.back-btn {
			display: inline-flex;
			align-items: center;
			justify-content: center;
			background: transparent;
			color: var(--text);
			padding: 0.3rem;
		}
		.back-btn:hover:not(:disabled) {
			background: var(--surface-2);
		}
		.messages {
			padding: 1rem 0.8rem;
		}
		.send-label {
			display: none;
		}
	}
</style>
