// End-to-end smoke test against a running ChatBox node.
// Usage: node e2e.mjs http://127.0.0.1:18080

const base = process.argv[2] ?? 'http://127.0.0.1:18080';

async function call(method, path, { token, body } = {}) {
	const res = await fetch(`${base}/api/v1${path}`, {
		method,
		headers: {
			'content-type': 'application/json',
			...(token ? { authorization: `Bearer ${token}` } : {})
		},
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	const text = await res.text();
	const json = text ? JSON.parse(text) : null;
	if (!res.ok) throw new Error(`${method} ${path} -> ${res.status}: ${text}`);
	return json;
}

function assert(condition, message) {
	if (!condition) throw new Error(`assertion failed: ${message}`);
}

const suffix = Date.now().toString().slice(-8);
const aliceId = `alice${suffix}`;
const bobId = `bob${suffix}`;

const alice = await call('POST', '/auth/register', {
	body: { user_id: aliceId, password: 'password123', device_name: 'web', platform: 'web' }
});
const bob = await call('POST', '/auth/register', {
	body: { user_id: bobId, password: 'password123', device_name: 'android', platform: 'android' }
});
console.log('registered', alice.user.user_id, bob.user.user_id);

// Bob connects over WebSocket and authenticates.
const received = [];
const bobReady = new Promise((resolve, reject) => {
	const ws = new WebSocket(`${base.replace('http', 'ws')}/ws`);
	const timer = setTimeout(() => reject(new Error('bob did not become ready')), 5000);
	ws.onopen = () => {
		ws.send(
			JSON.stringify({
				type: 'auth',
				token: bob.tokens.access_token,
				device_id: bob.tokens.device_id
			})
		);
	};
	ws.onmessage = (event) => {
		const frame = JSON.parse(event.data);
		if (frame.type === 'ready') {
			clearTimeout(timer);
			resolve(ws);
		} else {
			received.push(frame);
		}
	};
	ws.onerror = (e) => reject(new Error(`websocket error: ${e.message ?? e}`));
});

const ws = await bobReady;
console.log('bob websocket ready');

// Alice opens a direct chat with Bob and sends a message over REST.
const direct = await call('POST', '/conversations/direct', {
	token: alice.tokens.access_token,
	body: { user_id: bobId }
});
const conversationId = direct.conversation.id;
await call('POST', `/conversations/${conversationId}/messages`, {
	token: alice.tokens.access_token,
	body: { client_msg_id: 'e2e-1', kind: 'text', content: 'hello from alice' }
});

// Wait for the fan-out to reach Bob's WebSocket.
const deadline = Date.now() + 5000;
while (Date.now() < deadline) {
	if (received.some((f) => f.type === 'message' && f.message.content === 'hello from alice')) break;
	await new Promise((r) => setTimeout(r, 50));
}
const delivered = received.find(
	(f) => f.type === 'message' && f.message.content === 'hello from alice'
);
assert(delivered, 'bob receives the fanned-out message');
console.log('fan-out delivered message to bob');

// Bob replies over the WebSocket; Alice reads it over REST.
ws.send(
	JSON.stringify({
		type: 'send',
		client_msg_id: 'e2e-2',
		conversation_id: conversationId,
		kind: 'text',
		content: 'hi alice, got it'
	})
);
let ack;
const ackDeadline = Date.now() + 5000;
while (Date.now() < ackDeadline) {
	ack = received.find((f) => f.type === 'ack' && f.client_msg_id === 'e2e-2');
	if (ack) break;
	await new Promise((r) => setTimeout(r, 50));
}
assert(ack, 'bob receives an ack for his websocket send');

const history = await call('GET', `/conversations/${conversationId}/messages?limit=10`, {
	token: alice.tokens.access_token
});
assert(history.messages.length === 2, 'conversation has two messages');
assert(
	history.messages.some((m) => m.content === 'hi alice, got it'),
	'alice sees bob\u2019s reply'
);
console.log('websocket send persisted and readable over REST');

// Search verifies content search and sorting.
const search = await call('GET', '/search?q=hello&sort=asc', {
	token: alice.tokens.access_token
});
assert(search.results.length === 1, 'content search finds exactly one message');
assert(search.results[0].content === 'hello from alice', 'search content matches');
console.log('search works');

ws.close();

// File transfer: pre-sign, upload, complete, send, download.
const presign = await call('POST', '/files/presign', {
	token: alice.tokens.access_token,
	body: { filename: 'clip.mp4', content_type: 'video/mp4', size: 4 }
});
const uploadResponse = await fetch(presign.upload.upload_url, {
	method: 'PUT',
	headers: { authorization: `Bearer ${alice.tokens.access_token}` },
	body: Buffer.from('data')
});
assert(uploadResponse.ok, 'upload succeeds');
const complete = await call('POST', `/files/${presign.upload.attachment_id}/complete`, {
	token: alice.tokens.access_token,
	body: { filename: 'clip.mp4', content_type: 'video/mp4' }
});
assert(complete.attachment.size === 4, 'attachment size is recorded');
assert(complete.attachment.sha256.length === 64, 'attachment hash is recorded');
await call('POST', `/conversations/${conversationId}/messages`, {
	token: alice.tokens.access_token,
	body: { client_msg_id: 'e2e-file', kind: 'file', attachment_id: complete.attachment.id }
});
const download = await fetch(`${base}/api/v1/files/${complete.attachment.id}/content`, {
	headers: { authorization: `Bearer ${alice.tokens.access_token}` }
});
assert(download.ok, 'attachment content is downloadable');
assert((await download.text()) === 'data', 'attachment bytes round-trip');
console.log('file transfer works');

// Type search by filename wildcard.
const fileSearch = await call('GET', '/search?filename=*.mp4&kind=file', {
	token: alice.tokens.access_token
});
assert(fileSearch.results.length === 1, 'wildcard filename search finds the file');

// Time-range search (minute precision).
const now = new Date();
const from = new Date(now.getTime() - 60_000).toISOString();
const to = new Date(now.getTime() + 60_000).toISOString();
const timeSearch = await call(
	'GET',
	`/search?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&sort=desc`,
	{ token: alice.tokens.access_token }
);
assert(timeSearch.results.length >= 3, 'time-range search returns the conversation history');
assert(
	timeSearch.results[0].seq > timeSearch.results.at(-1).seq,
	'descending sort returns newest first'
);
console.log('wildcard and time-range search work');

console.log('E2E PASS');
