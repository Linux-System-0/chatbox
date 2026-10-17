<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import Avatar from '$lib/Avatar.svelte';
	import { deleteJson, getJson, patchJson, postJson } from '$lib/api';
	import { effectiveAvatar, effectiveName, fileToAvatar } from '$lib/avatar';
	import { translator } from '$lib/i18n';
	import { loadSession, session, type User } from '$lib/session';
	import { ChatSocket, type ChatMessage, type ConnectionStatus, type ServerFrame } from '$lib/ws';

	interface ConversationView {
		id: number;
		kind: 'direct' | 'group';
		title: string;
		avatar_url?: string | null;
		peer_id?: number | null;
		group_id?: number | null;
		member_count: number;
		last_message: ChatMessage | null;
		last_read_seq: number;
		unread: number;
	}
	interface ContactView {
		user: User;
		remark: string | null;
		remark_avatar_url: string | null;
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
		created_at: string;
		from_user: User | null;
		to_user: User | null;
	}
	interface GroupMemberView {
		user: User;
		role: 'owner' | 'admin' | 'member';
		muted_until: string | null;
		joined_at: string;
	}
	interface GroupInfo {
		id: number;
		conversation_id: number;
		name: string;
		owner_id: number;
		invite_policy: 'owner' | 'admin' | 'member';
		created_at: string;
	}
	interface GroupView {
		group: GroupInfo;
		members: GroupMemberView[];
	}
	interface MessageItem {
		message: ChatMessage;
		mine: boolean;
		head: boolean;
		tail: boolean;
		day: string | null;
	}
	interface PresenceEntry {
		user_id: number;
		online: boolean;
	}

	$: tr = $translator;

	let conversations: ConversationView[] = [];
	let contacts: ContactView[] = [];
	let requests: FriendRequest[] = [];
	let blocks: BlockView[] = [];
	let messages: ChatMessage[] = [];
	let presence: Record<number, boolean> = {};
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

	let contactEditor: ContactView | null = null;
	let contactRemark = '';
	let contactAvatar = '';
	let contactBusy = false;
	let contactError = '';

	let showNewGroup = false;
	let groupName = '';
	let groupSelection: number[] = [];
	let groupBusy = false;
	let groupError = '';

	let showGroupInfo = false;
	let groupInfo: GroupView | null = null;
	let groupRename = '';
	let groupAddSelection: number[] = [];
	let groupActionError = '';

	$: myId = $session?.user.id;
	$: selectedConversation = conversations.find((c) => c.id === selected) ?? null;
	$: pendingRequests = requests.filter((r) => r.status === 'pending');
	$: incomingRequests = pendingRequests.filter((r) => r.to_id === myId);
	$: outgoingRequests = pendingRequests.filter((r) => r.from_id === myId);
	$: messageItems = buildMessageItems(messages, myId, tr);
	$: myGroupMember = groupInfo?.members.find((member) => member.user.id === myId) ?? null;
	$: canManageGroup =
		myGroupMember?.role === 'owner' || myGroupMember?.role === 'admin';
	$: isGroupOwner = myGroupMember?.role === 'owner';
	$: groupAddable = contacts.filter(
		(contact) => !groupInfo?.members.some((member) => member.user.id === contact.user.id)
	);

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

	function previewOf(conversation: ConversationView): string {
		const message = conversation.last_message;
		return message ? contentOf(message) : '';
	}

	function contentOf(message: ChatMessage): string {
		return message.content ?? (message.kind === 'file' ? tr('file') : `[${message.kind}]`);
	}

	function identityFor(userId: number): { name: string; avatar: string | null } {
		const contact = contacts.find((item) => item.user.id === userId);
		if (contact) {
			return {
				name: effectiveName(contact.remark, contact.user.display_name, contact.user.user_id),
				avatar: effectiveAvatar(contact.remark_avatar_url, contact.user.avatar_url)
			};
		}
		const member = groupInfo?.members.find((item) => item.user.id === userId);
		if (member) {
			return {
				name: effectiveName(null, member.user.display_name, member.user.user_id),
				avatar: effectiveAvatar(null, member.user.avatar_url)
			};
		}
		if ($session?.user.id === userId) {
			return {
				name: effectiveName(null, $session.user.display_name, $session.user.user_id),
				avatar: effectiveAvatar(null, $session.user.avatar_url)
			};
		}
		return { name: `#${userId}`, avatar: null };
	}

	function isOnline(userId: number | null | undefined): boolean {
		return userId !== null && userId !== undefined && presence[userId] === true;
	}

	$: toolbarStatus =
		selectedConversation?.kind === 'direct' && selectedConversation.peer_id
			? status === 'online'
				? presence[selectedConversation.peer_id]
					? 'online'
					: 'offline'
				: status
			: '';

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
			await refreshPresence();
			if (selected !== null) {
				const conversation = conversations.find((item) => item.id === selected);
				if (conversation?.kind === 'group') await loadGroupInfo(conversation);
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		}
	}

	async function refreshPresence() {
		const ids = new Set<number>();
		for (const contact of contacts) ids.add(contact.user.id);
		for (const conversation of conversations) {
			if (conversation.peer_id) ids.add(conversation.peer_id);
		}
		for (const member of groupInfo?.members ?? []) ids.add(member.user.id);
		if (ids.size === 0) return;
		try {
			const response = await getJson<{ presence: PresenceEntry[] }>(
				`/presence?user_ids=${[...ids].join(',')}`
			);
			const next: Record<number, boolean> = { ...presence };
			for (const entry of response.presence) next[entry.user_id] = entry.online;
			presence = next;
		} catch {
			// Presence is auxiliary; a failure must not break the workspace.
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
			if (conv?.kind === 'group') {
				await loadGroupInfo(conv);
				showGroupInfo = false;
			} else {
				groupInfo = null;
				showGroupInfo = false;
			}
			await refreshPresence();
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
		} else if (frame.type === 'presence') {
			presence = { ...presence, [frame.user_id]: frame.online };
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
		const contact = contacts.find((item) => item.user.user_id === newDirect);
		if (!contact) {
			error = tr('contact_only_hint');
			return;
		}
		await chatWith(contact);
	}

	async function chatWith(contact: ContactView) {
		try {
			const response = await postJson<{ conversation: { id: number } }>('/conversations/direct', {
				user_id: contact.user.user_id
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
		try {
			await postJson(`/contacts/requests/${id}/accept`);
			await refreshAll();
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to accept request';
		}
	}

	async function rejectRequest(id: number) {
		try {
			await postJson(`/contacts/requests/${id}/reject`);
			await refreshAll();
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to reject request';
		}
	}

	async function blockUser(user: string) {
		const id = user.trim();
		if (!id) return;
		try {
			await postJson(`/blocks/${encodeURIComponent(id)}`);
			await refreshAll();
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to block user';
		}
	}

	async function unblock(user_id: string) {
		try {
			await deleteJson(`/blocks/${encodeURIComponent(user_id)}`);
			await refreshAll();
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to unblock user';
		}
	}

	function openContactEditor(contact: ContactView) {
		contactEditor = contact;
		contactRemark = contact.remark ?? '';
		contactAvatar = contact.remark_avatar_url ?? '';
		contactError = '';
	}

	async function chooseContactAvatar(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		contactBusy = true;
		contactError = '';
		try {
			contactAvatar = await fileToAvatar(file);
		} catch {
			contactError = tr('avatar_error');
		} finally {
			contactBusy = false;
			input.value = '';
		}
	}

	async function saveContactEditor() {
		if (!contactEditor) return;
		contactBusy = true;
		contactError = '';
		try {
			await patchJson(`/contacts/${encodeURIComponent(contactEditor.user.user_id)}`, {
				remark: contactRemark.trim() || null,
				remark_avatar_url: contactAvatar || null
			});
			contactEditor = null;
			await refreshAll();
		} catch (e) {
			contactError = e instanceof Error ? e.message : 'failed to update contact';
		} finally {
			contactBusy = false;
		}
	}

	function openNewGroup() {
		showNewGroup = true;
		groupName = '';
		groupSelection = [];
		groupError = '';
	}

	function toggleGroupSelection(userId: number) {
		groupSelection = groupSelection.includes(userId)
			? groupSelection.filter((id) => id !== userId)
			: [...groupSelection, userId];
	}

	async function createGroup() {
		const name = groupName.trim();
		if (!name) {
			groupError = tr('group_name');
			return;
		}
		if (groupSelection.length === 0) {
			groupError = tr('no_contacts_selected');
			return;
		}
		groupBusy = true;
		groupError = '';
		try {
			const members = contacts
				.filter((contact) => groupSelection.includes(contact.user.id))
				.map((contact) => contact.user.user_id);
			const response = await postJson<{ group: GroupView }>('/groups', { name, members });
			showNewGroup = false;
			groupInfo = response.group;
			await refreshAll();
			await selectConversation(response.group.group.conversation_id);
			showGroupInfo = true;
		} catch (e) {
			groupError = e instanceof Error ? e.message : tr('group_error');
		} finally {
			groupBusy = false;
		}
	}

	async function loadGroupInfo(conversation: ConversationView) {
		if (!conversation.group_id) {
			groupInfo = null;
			return;
		}
		try {
			const response = await getJson<{ group: GroupView }>(
				`/groups/${conversation.group_id}`
			);
			groupInfo = response.group;
			groupRename = response.group.group.name;
			groupAddSelection = [];
			groupActionError = '';
		} catch (e) {
			groupInfo = null;
			groupActionError = e instanceof Error ? e.message : tr('group_error');
		}
	}

	async function openGroupInfo() {
		if (!selectedConversation) return;
		await loadGroupInfo(selectedConversation);
		showGroupInfo = true;
		await refreshPresence();
	}

	async function renameGroup() {
		if (!groupInfo) return;
		const name = groupRename.trim();
		if (!name) return;
		try {
			await patchJson(`/groups/${groupInfo.group.id}`, { name });
			await loadGroupInfo(selectedConversation as ConversationView);
			await refreshAll();
		} catch (e) {
			groupActionError = e instanceof Error ? e.message : tr('group_error');
		}
	}

	async function addGroupMembers() {
		if (!groupInfo || groupAddSelection.length === 0) return;
		try {
			const members = contacts
				.filter((contact) => groupAddSelection.includes(contact.user.id))
				.map((contact) => contact.user.user_id);
			await postJson(`/groups/${groupInfo.group.id}/members`, { members });
			await loadGroupInfo(selectedConversation as ConversationView);
			await refreshAll();
		} catch (e) {
			groupActionError = e instanceof Error ? e.message : tr('group_error');
		}
	}

	async function kickGroupMember(user: User) {
		if (!groupInfo) return;
		try {
			await deleteJson(`/groups/${groupInfo.group.id}/members/${encodeURIComponent(user.user_id)}`);
			await loadGroupInfo(selectedConversation as ConversationView);
			await refreshAll();
		} catch (e) {
			groupActionError = e instanceof Error ? e.message : tr('group_error');
		}
	}

	async function toggleMute(member: GroupMemberView) {
		if (!groupInfo) return;
		try {
			const muted = member.muted_until ? new Date(member.muted_until).getTime() > Date.now() : false;
			await postJson(
				`/groups/${groupInfo.group.id}/members/${encodeURIComponent(member.user.user_id)}/mute`,
				{ minutes: muted ? null : 60 }
			);
			await loadGroupInfo(selectedConversation as ConversationView);
		} catch (e) {
			groupActionError = e instanceof Error ? e.message : tr('group_error');
		}
	}

	async function leaveGroup() {
		if (!groupInfo) return;
		if (!confirm(tr('confirm_leave_group'))) return;
		try {
			await postJson(`/groups/${groupInfo.group.id}/leave`);
			closeGroup();
		} catch (e) {
			groupActionError = e instanceof Error ? e.message : tr('group_error');
		}
	}

	async function dissolveGroup() {
		if (!groupInfo) return;
		if (!confirm(tr('confirm_dissolve_group'))) return;
		try {
			await deleteJson(`/groups/${groupInfo.group.id}`);
			closeGroup();
		} catch (e) {
			groupActionError = e instanceof Error ? e.message : tr('group_error');
		}
	}

	function closeGroup() {
		showGroupInfo = false;
		groupInfo = null;
		selected = null;
		messages = [];
		refreshAll();
	}

	function roleLabel(role: GroupMemberView['role']): string {
		return tr(role);
	}

	function toggleGroupAdd(userId: number) {
		groupAddSelection = groupAddSelection.includes(userId)
			? groupAddSelection.filter((id) => id !== userId)
			: [...groupAddSelection, userId];
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
				<div class="new-row">
					<select bind:value={newDirect} aria-label={tr('select_contact')}>
						<option value="">{tr('select_contact')}</option>
						{#each contacts as contact (contact.user.id)}
							<option value={contact.user.user_id}>
								{effectiveName(contact.remark, contact.user.display_name, contact.user.user_id)}
							</option>
						{/each}
					</select>
					<button
						type="button"
						on:click={startDirect}
						title={tr('start_chat')}
						aria-label={tr('start_chat')}>+</button
					>
					<button
						type="button"
						class="ghost"
						on:click={openNewGroup}
						title={tr('new_group')}
						aria-label={tr('new_group')}>👥</button
					>
				</div>
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
									<Avatar
										src={conversation.avatar_url}
										name={conversation.title}
										id={conversation.peer_id ?? conversation.id}
									/>
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

				{#if incomingRequests.length > 0}
					<h2>{tr('incoming_requests')}</h2>
					<ul class="list">
						{#each incomingRequests as request (request.id)}
							<li class="contact-row">
								<Avatar
									src={request.from_user?.avatar_url}
									name={effectiveName(
										null,
										request.from_user?.display_name,
										request.from_user?.user_id ?? String(request.from_id)
									)}
									id={request.from_id}
									size="sm"
								/>
								<span class="contact-main">
									<span class="contact-name">
										{effectiveName(
											null,
											request.from_user?.display_name,
											request.from_user?.user_id ?? `#${request.from_id}`
										)}
									</span>
									{#if request.message}<span class="contact-note">{request.message}</span>{/if}
								</span>
								<button class="chip" on:click={() => acceptRequest(request.id)}>{tr('accept')}</button>
								<button class="chip subtle" on:click={() => rejectRequest(request.id)}
									>{tr('reject')}</button
								>
							</li>
						{/each}
					</ul>
				{/if}

				{#if outgoingRequests.length > 0}
					<h2>{tr('outgoing_requests')}</h2>
					<ul class="list">
						{#each outgoingRequests as request (request.id)}
							<li class="contact-row">
								<Avatar
									src={request.to_user?.avatar_url}
									name={effectiveName(
										null,
										request.to_user?.display_name,
										request.to_user?.user_id ?? String(request.to_id)
									)}
									id={request.to_id}
									size="sm"
								/>
								<span class="contact-name">
									{effectiveName(
										null,
										request.to_user?.display_name,
										request.to_user?.user_id ?? `#${request.to_id}`
									)}
								</span>
								<span class="chip subtle pending-chip">{tr('pending')}</span>
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
								<Avatar
									src={effectiveAvatar(contact.remark_avatar_url, contact.user.avatar_url)}
									name={effectiveName(contact.remark, contact.user.display_name, contact.user.user_id)}
									id={contact.user.id}
									size="sm"
								/>
								<span class="contact-main">
									<span class="contact-name">
										{effectiveName(contact.remark, contact.user.display_name, contact.user.user_id)}
									</span>
									<span class="contact-note">@{contact.user.user_id}</span>
								</span>
								<span
									class="online-dot"
									class:on={isOnline(contact.user.id)}
									title={isOnline(contact.user.id) ? tr('online') : tr('offline')}
									aria-label={isOnline(contact.user.id) ? tr('online') : tr('offline')}
								></span>
								<button
									class="chip"
									title={tr('chat')}
									on:click={() => chatWith(contact)}>{tr('chat')}</button
								>
								<button
									class="chip subtle"
									title={tr('edit_contact')}
									on:click={() => openContactEditor(contact)}>✎</button
								>
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
								<Avatar
									src={block.user.avatar_url}
									name={effectiveName(null, block.user.display_name, block.user.user_id)}
									id={block.user.id}
									size="sm"
									muted
								/>
								<span class="contact-main">
									<span class="contact-name">
										{effectiveName(null, block.user.display_name, block.user.user_id)}
									</span>
									<span class="contact-note">@{block.user.user_id}</span>
								</span>
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
				<Avatar
					src={selectedConversation.avatar_url}
					name={selectedConversation.title}
					id={selectedConversation.peer_id ?? selectedConversation.id}
				/>
				<div class="toolbar-title">
					<strong>{selectedConversation.title}</strong>
					{#if selectedConversation.kind === 'group'}
						<button class="group-meta" on:click={openGroupInfo}>
							{selectedConversation.member_count} {tr('members')}
						</button>
					{:else}
						<span class="status-pill {toolbarStatus}">
							<i class="status-dot"></i>
							{tr(toolbarStatus)}
						</span>
					{/if}
				</div>
				{#if selectedConversation.kind === 'group'}
					<button class="ghost info-btn" on:click={openGroupInfo}>{tr('group_info')}</button>
				{/if}
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
							<span class="msg-avatar-slot" class:invisible={!item.head}>
								<Avatar
									src={identityFor(item.message.sender_id).avatar}
									name={identityFor(item.message.sender_id).name}
									id={item.message.sender_id}
									size="sm"
								/>
							</span>
						{/if}
						<div class="bubble">
							{#if selectedConversation?.kind === 'group' && !item.mine && item.head}
								<span class="sender">{identityFor(item.message.sender_id).name}</span>
							{/if}
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


{#if contactEditor}
	<div class="overlay" role="presentation">
		<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="contact-editor-title">
			<header>
				<h2 id="contact-editor-title">{tr('edit_contact')}</h2>
				<button class="ghost close" on:click={() => (contactEditor = null)} aria-label={tr('close')}
					>×</button
				>
			</header>

			<div class="avatar-row">
				<Avatar
					src={contactAvatar || contactEditor.user.avatar_url}
					name={effectiveName(
						contactRemark,
						contactEditor.user.display_name,
						contactEditor.user.user_id
					)}
					id={contactEditor.user.id}
					size="lg"
				/>
				<div class="avatar-actions">
					<label class="upload">
						<span>{tr('remark_avatar')}</span>
						<input type="file" accept="image/*" on:change={chooseContactAvatar} disabled={contactBusy} />
					</label>
					{#if contactAvatar}
						<button type="button" class="ghost" on:click={() => (contactAvatar = '')}
							>{tr('remove_avatar')}</button
						>
					{/if}
				</div>
			</div>

			<label class="field">
				<span>{tr('remark')}</span>
				<input bind:value={contactRemark} maxlength="64" placeholder={tr('remark')} />
			</label>
			<p class="hint">@{contactEditor.user.user_id}</p>

			{#if contactError}<p class="err" role="alert">{contactError}</p>{/if}

			<div class="actions">
				<button type="button" class="ghost" on:click={() => (contactEditor = null)}>{tr('cancel')}</button>
				<button type="button" on:click={saveContactEditor} disabled={contactBusy}>
					{contactBusy ? '…' : tr('save')}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if showNewGroup}
	<div class="overlay" role="presentation">
		<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="new-group-title">
			<header>
				<h2 id="new-group-title">{tr('new_group')}</h2>
				<button class="ghost close" on:click={() => (showNewGroup = false)} aria-label={tr('close')}
					>×</button
				>
			</header>

			<label class="field">
				<span>{tr('group_name')}</span>
				<input bind:value={groupName} maxlength="64" placeholder={tr('group_name')} />
			</label>

			<h3>{tr('select_contacts')}</h3>
			{#if contacts.length === 0}
				<p class="empty">{tr('no_contacts')}</p>
			{:else}
				<ul class="check-list">
					{#each contacts as contact (contact.user.id)}
						<li>
							<label>
								<input
									type="checkbox"
									checked={groupSelection.includes(contact.user.id)}
									on:change={() => toggleGroupSelection(contact.user.id)}
								/>
								<Avatar
									src={effectiveAvatar(contact.remark_avatar_url, contact.user.avatar_url)}
									name={effectiveName(contact.remark, contact.user.display_name, contact.user.user_id)}
									id={contact.user.id}
									size="sm"
								/>
								<span>{effectiveName(contact.remark, contact.user.display_name, contact.user.user_id)}</span>
							</label>
						</li>
					{/each}
				</ul>
			{/if}

			{#if groupError}<p class="err" role="alert">{groupError}</p>{/if}

			<div class="actions">
				<button type="button" class="ghost" on:click={() => (showNewGroup = false)}>{tr('cancel')}</button>
				<button type="button" on:click={createGroup} disabled={groupBusy}>
					{groupBusy ? '…' : tr('create_group')}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if showGroupInfo && groupInfo}
	<div class="overlay" role="presentation">
		<div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="group-info-title">
			<header>
				<Avatar src={null} name={groupInfo.group.name} id={groupInfo.group.id} />
				<h2 id="group-info-title">{groupInfo.group.name}</h2>
				<button class="ghost close" on:click={() => (showGroupInfo = false)} aria-label={tr('close')}
					>×</button
				>
			</header>

			{#if groupActionError}<p class="err" role="alert">{groupActionError}</p>{/if}

			{#if canManageGroup}
				<div class="inline-form">
					<input bind:value={groupRename} maxlength="64" aria-label={tr('group_name')} />
					<button type="button" on:click={renameGroup}>{tr('rename_group')}</button>
				</div>
			{/if}

			<h3>{tr('members')}</h3>
			<ul class="member-list">
				{#each groupInfo.members as member (member.user.id)}
					<li>
						<Avatar
							src={identityFor(member.user.id).avatar}
							name={identityFor(member.user.id).name}
							id={member.user.id}
							size="sm"
						/>
						<span class="contact-main">
							<span class="contact-name">
								{identityFor(member.user.id).name}
								{#if member.user.id === myId}<span class="muted-label">({tr('you')})</span>{/if}
							</span>
							<span class="contact-note">
								{roleLabel(member.role)}
								{#if member.muted_until && new Date(member.muted_until).getTime() > Date.now()}
									· {tr('mute')}
								{/if}
							</span>
						</span>
						<span
							class="online-dot"
							class:on={isOnline(member.user.id)}
							title={isOnline(member.user.id) ? tr('online') : tr('offline')}
						></span>
						{#if canManageGroup && member.role !== 'owner' && member.user.id !== myId && (isGroupOwner || member.role === 'member')}
							<button class="chip subtle" on:click={() => toggleMute(member)}>
								{member.muted_until && new Date(member.muted_until).getTime() > Date.now()
									? tr('unmute')
									: tr('mute')}
							</button>
							<button class="chip subtle" on:click={() => kickGroupMember(member.user)}>{tr('kick')}</button>
						{/if}
					</li>
				{/each}
			</ul>

			{#if canManageGroup && groupAddable.length > 0}
				<h3>{tr('add_members')}</h3>
				<ul class="check-list">
					{#each groupAddable as contact (contact.user.id)}
						<li>
							<label>
								<input
									type="checkbox"
									checked={groupAddSelection.includes(contact.user.id)}
									on:change={() => toggleGroupAdd(contact.user.id)}
								/>
								<Avatar
									src={effectiveAvatar(contact.remark_avatar_url, contact.user.avatar_url)}
									name={effectiveName(contact.remark, contact.user.display_name, contact.user.user_id)}
									id={contact.user.id}
									size="sm"
								/>
								<span>{effectiveName(contact.remark, contact.user.display_name, contact.user.user_id)}</span>
							</label>
						</li>
					{/each}
				</ul>
				<div class="actions">
					<button
						type="button"
						on:click={addGroupMembers}
						disabled={groupAddSelection.length === 0}>{tr('add_members')}</button
					>
				</div>
			{/if}

			<div class="actions split">
				{#if !isGroupOwner || groupInfo.members.length === 1}
					<button type="button" class="ghost" on:click={leaveGroup}>{tr('leave_group')}</button>
				{:else}
					<span></span>
				{/if}
				{#if isGroupOwner}
					<button type="button" class="danger" on:click={dissolveGroup}>{tr('dissolve_group')}</button>
				{/if}
			</div>
		</div>
	</div>
{/if}

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

	.new-row select {
		flex: 1;
		min-width: 0;
		padding: 0.5rem 0.65rem;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--surface);
		color: var(--text);
		font: inherit;
		font-size: 0.9rem;
	}
	.contact-main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
	}
	.contact-note {
		font-size: 0.72rem;
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.contact-row .chip {
		padding: 0.2rem 0.5rem;
	}
	.pending-chip {
		pointer-events: none;
		opacity: 0.8;
	}
	.online-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		flex: none;
		border-radius: 50%;
		background: var(--muted);
		opacity: 0.5;
	}
	.online-dot.on {
		background: var(--success);
		opacity: 1;
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--success) 18%, transparent);
	}
	.group-meta {
		align-self: flex-start;
		background: transparent;
		color: var(--muted);
		padding: 0;
		font-size: 0.72rem;
		font-weight: 400;
		border-radius: 0;
	}
	.group-meta:hover:not(:disabled) {
		background: transparent;
		color: var(--text);
		text-decoration: underline;
	}
	.info-btn {
		margin-left: auto;
		padding: 0.35rem 0.6rem;
		font-size: 0.78rem;
	}
	.msg-avatar-slot {
		flex: none;
		margin-bottom: 0.15rem;
	}
	.msg-avatar-slot.invisible {
		visibility: hidden;
	}
	.sender {
		display: block;
		margin-bottom: 0.15rem;
		font-size: 0.72rem;
		font-weight: 700;
		color: var(--accent);
	}
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(8, 10, 16, 0.55);
		display: grid;
		place-items: center;
		padding: 1rem;
		z-index: 65;
	}
	.dialog {
		width: min(32rem, 100%);
		max-height: min(85vh, 44rem);
		overflow: auto;
		background: var(--surface);
		color: var(--text);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-md);
		padding: 1.1rem 1.2rem 1.3rem;
	}
	.dialog.wide {
		width: min(38rem, 100%);
	}
	.dialog header {
		display: flex;
		align-items: center;
		gap: 0.65rem;
	}
	.dialog h2 {
		margin: 0;
		font-size: 1.02rem;
		flex: 1;
	}
	.dialog h3 {
		margin: 1rem 0 0.45rem;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--muted);
	}
	.dialog .close {
		padding: 0.25rem 0.55rem;
		font-size: 1.1rem;
		line-height: 1;
	}
	.avatar-row {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin: 1rem 0;
	}
	.avatar-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		align-items: center;
	}
	.upload {
		display: inline-flex;
		align-items: center;
		padding: 0.45rem 0.8rem;
		border-radius: var(--radius-sm);
		background: var(--accent);
		color: var(--accent-contrast);
		font-weight: 600;
		font-size: 0.82rem;
		cursor: pointer;
	}
	.upload input {
		position: absolute;
		width: 1px;
		height: 1px;
		opacity: 0;
		pointer-events: none;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		margin-bottom: 0.8rem;
		font-size: 0.82rem;
		font-weight: 600;
	}
	.field input {
		width: 100%;
	}
	.hint {
		margin: -0.3rem 0 0.8rem;
		font-size: 0.76rem;
		color: var(--muted);
	}
	.inline-form {
		display: flex;
		gap: 0.45rem;
		margin: 0.8rem 0 0.2rem;
	}
	.inline-form input {
		flex: 1;
		min-width: 0;
	}
	.check-list,
	.member-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.check-list li label {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.35rem 0.45rem;
		border-radius: var(--radius-sm);
		font-size: 0.86rem;
		cursor: pointer;
	}
	.check-list li label:hover {
		background: var(--surface-2);
	}
	.check-list input[type='checkbox'] {
		accent-color: var(--accent);
	}
	.member-list li {
		display: flex;
		align-items: center;
		gap: 0.55rem;
		padding: 0.35rem 0.45rem;
		border-radius: var(--radius-sm);
	}
	.member-list li:hover {
		background: var(--surface-2);
	}
	.muted-label {
		color: var(--muted);
		font-weight: 400;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 0.5rem;
		margin-top: 0.9rem;
	}
	.actions.split {
		justify-content: space-between;
	}
	.err {
		margin: 0.6rem 0;
		padding: 0.5rem 0.65rem;
		border-radius: var(--radius-sm);
		background: var(--danger-bg);
		color: var(--danger-text);
		border: 1px solid var(--danger-border);
		font-size: 0.82rem;
	}
	.danger {
		background: var(--danger-text);
		color: #fff;
	}
	.danger:hover:not(:disabled) {
		background: color-mix(in srgb, var(--danger-text) 85%, #000);
	}
	@media (max-width: 760px) {
		.dialog {
			width: 100%;
			max-height: 90vh;
		}
		.contact-row .chip.subtle {
			display: none;
		}
	}

</style>
