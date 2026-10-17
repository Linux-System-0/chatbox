import type { Session } from './session';

export interface ChatMessage {
	id: number;
	conversation_id: number;
	seq: number;
	sender_id: number;
	client_msg_id: string;
	kind: 'text' | 'file' | 'system';
	content?: string | null;
	attachment_id?: number | null;
	mentions: number[];
	created_at: string;
}

export type ServerFrame =
	| { type: 'ready'; server_time: string }
	| { type: 'ack'; client_msg_id: string; conversation_id: number; seq: number; message_id: number }
	| { type: 'message'; message: ChatMessage }
	| { type: 'message_hint'; conversation_id: number; seq: number }
	| { type: 'presence'; user_id: number; online: boolean; devices: number }
	| { type: 'pong' }
	| { type: 'error'; code: string; message: string };

export type ConnectionStatus = 'connecting' | 'online' | 'offline';

/** A reconnecting WebSocket client for the realtime data plane. */
export class ChatSocket {
	private socket?: WebSocket;
	private heartbeat?: ReturnType<typeof setInterval>;
	private reconnectDelay = 500;
	private closed = false;

	constructor(
		private readonly session: Session,
		private readonly onFrame: (frame: ServerFrame) => void,
		private readonly onStatus: (status: ConnectionStatus) => void
	) {}

	connect(): void {
		if (typeof WebSocket === 'undefined') return;
		this.closed = false;
		this.onStatus('connecting');
		const protocol = location.protocol === 'https:' ? 'wss' : 'ws';
		const socket = new WebSocket(`${protocol}://${location.host}/ws`);
		this.socket = socket;

		socket.onopen = () => {
			socket.send(
				JSON.stringify({
					type: 'auth',
					token: this.session.accessToken,
					device_id: this.session.deviceId
				})
			);
		};

		socket.onmessage = (event) => {
			let frame: ServerFrame;
			try {
				frame = JSON.parse(event.data as string) as ServerFrame;
			} catch {
				return;
			}
			if (frame.type === 'ready') {
				this.reconnectDelay = 500;
				this.onStatus('online');
				this.heartbeat = setInterval(() => this.send({ type: 'ping' }), 25000);
			}
			this.onFrame(frame);
		};

		socket.onclose = () => {
			if (this.heartbeat) clearInterval(this.heartbeat);
			this.onStatus('offline');
			if (!this.closed) {
				setTimeout(() => this.connect(), this.reconnectDelay);
				this.reconnectDelay = Math.min(this.reconnectDelay * 2, 15000);
			}
		};

		socket.onerror = () => socket.close();
	}

	send(frame: Record<string, unknown>): void {
		if (this.socket?.readyState === WebSocket.OPEN) {
			this.socket.send(JSON.stringify(frame));
		}
	}

	close(): void {
		this.closed = true;
		if (this.heartbeat) clearInterval(this.heartbeat);
		this.socket?.close();
	}
}
