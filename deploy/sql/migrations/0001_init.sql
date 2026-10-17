-- ChatBox initial schema.
--
-- Messages are sharded by conversation_id; the uniqueness constraints on
-- (conversation_id, seq) and (sender_id, client_msg_id) stay on the same shard.
-- The outbox table lives next to messages so the append and the publish intent
-- commit in one transaction (see docs/adr/0004-transactional-outbox.md).

CREATE TABLE IF NOT EXISTS users (
    id            BIGINT       NOT NULL,
    user_id       VARCHAR(64)  NOT NULL,
    display_name  VARCHAR(64)  NULL,
    password_hash VARCHAR(255) NOT NULL,
    created_at    DATETIME(6)  NOT NULL,
    updated_at    DATETIME(6)  NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uq_users_user_id (user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS devices (
    id          BIGINT      NOT NULL,
    user_id     BIGINT      NOT NULL,
    name        VARCHAR(64) NOT NULL,
    platform    VARCHAR(16) NOT NULL,
    last_seen_at DATETIME(6) NOT NULL,
    created_at  DATETIME(6) NOT NULL,
    PRIMARY KEY (id),
    KEY idx_devices_user (user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS sessions (
    id           VARCHAR(64)  NOT NULL,
    user_id      BIGINT       NOT NULL,
    device_id    BIGINT       NOT NULL,
    refresh_hash VARCHAR(64)  NOT NULL,
    issued_at    DATETIME(6)  NOT NULL,
    expires_at   DATETIME(6)  NOT NULL,
    revoked_at   DATETIME(6)  NULL,
    PRIMARY KEY (id),
    KEY idx_sessions_user (user_id),
    KEY idx_sessions_device (device_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS conversations (
    id         BIGINT      NOT NULL,
    kind       VARCHAR(16) NOT NULL,
    created_at DATETIME(6) NOT NULL,
    PRIMARY KEY (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS conversation_members (
    conversation_id BIGINT        NOT NULL,
    user_id         BIGINT        NOT NULL,
    last_read_seq   BIGINT UNSIGNED NOT NULL DEFAULT 0,
    muted_until     DATETIME(6)   NULL,
    joined_at       DATETIME(6)   NOT NULL,
    PRIMARY KEY (conversation_id, user_id),
    KEY idx_conv_members_user (user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS conversation_sequences (
    conversation_id BIGINT UNSIGNED NOT NULL,
    seq             BIGINT UNSIGNED NOT NULL DEFAULT 0,
    PRIMARY KEY (conversation_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS messages (
    id              BIGINT          NOT NULL,
    conversation_id BIGINT          NOT NULL,
    seq             BIGINT UNSIGNED NOT NULL,
    sender_id       BIGINT          NOT NULL,
    client_msg_id   VARCHAR(64)     NOT NULL,
    kind            VARCHAR(16)     NOT NULL,
    content         TEXT            NULL,
    attachment_id   BIGINT          NULL,
    mentions        JSON            NULL,
    created_at      DATETIME(6)     NOT NULL,
    edited_at       DATETIME(6)     NULL,
    deleted_at      DATETIME(6)     NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uq_messages_conv_seq (conversation_id, seq),
    UNIQUE KEY uq_messages_sender_client (sender_id, client_msg_id),
    KEY idx_messages_conv_created (conversation_id, created_at),
    FULLTEXT KEY ft_messages_content (content)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS attachments (
    id          BIGINT          NOT NULL,
    owner_id    BIGINT          NOT NULL,
    filename    VARCHAR(255)    NOT NULL,
    size        BIGINT UNSIGNED NOT NULL,
    mime        VARCHAR(127)    NOT NULL,
    sha256      CHAR(64)        NOT NULL,
    storage_key VARCHAR(255)    NOT NULL,
    created_at  DATETIME(6)     NOT NULL,
    PRIMARY KEY (id),
    KEY idx_attachments_owner (owner_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS contacts (
    owner_id   BIGINT      NOT NULL,
    contact_id BIGINT      NOT NULL,
    remark     VARCHAR(64) NULL,
    created_at DATETIME(6) NOT NULL,
    PRIMARY KEY (owner_id, contact_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS friend_requests (
    id           BIGINT      NOT NULL,
    from_id      BIGINT      NOT NULL,
    to_id        BIGINT      NOT NULL,
    message      TEXT        NULL,
    status       VARCHAR(16) NOT NULL,
    created_at   DATETIME(6) NOT NULL,
    responded_at DATETIME(6) NULL,
    PRIMARY KEY (id),
    KEY idx_friend_requests_from (from_id),
    KEY idx_friend_requests_to (to_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS chat_groups (
    id              BIGINT      NOT NULL,
    conversation_id BIGINT      NOT NULL,
    name            VARCHAR(64) NOT NULL,
    owner_id        BIGINT      NOT NULL,
    invite_policy   VARCHAR(16) NOT NULL,
    created_at      DATETIME(6) NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uq_chat_groups_conversation (conversation_id),
    KEY idx_chat_groups_owner (owner_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS group_members (
    group_id    BIGINT      NOT NULL,
    user_id     BIGINT      NOT NULL,
    role        VARCHAR(16) NOT NULL,
    muted_until DATETIME(6) NULL,
    joined_at   DATETIME(6) NOT NULL,
    PRIMARY KEY (group_id, user_id),
    KEY idx_group_members_user (user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS blocks (
    owner_id   BIGINT      NOT NULL,
    blocked_id BIGINT      NOT NULL,
    created_at DATETIME(6) NOT NULL,
    PRIMARY KEY (owner_id, blocked_id),
    KEY idx_blocks_blocked (blocked_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS outbox (
    id              BIGINT      NOT NULL,
    message_id      BIGINT      NOT NULL,
    conversation_id BIGINT      NOT NULL,
    payload         TEXT        NOT NULL,
    published_at    DATETIME(6) NULL,
    created_at      DATETIME(6) NOT NULL,
    PRIMARY KEY (id),
    KEY idx_outbox_pending (published_at, id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS plugin_states (
    name       VARCHAR(64) NOT NULL,
    enabled    TINYINT(1)  NOT NULL DEFAULT 0,
    updated_at DATETIME(6) NOT NULL,
    PRIMARY KEY (name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
