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
console.log('E2E PASS');
