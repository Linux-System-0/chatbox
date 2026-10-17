-- Server plugin key/value storage.
--
-- Keys are namespaced by plugin name by the plugin host. The table is global
-- (not sharded): plugin state is low-volume and lives on the first shard, see
-- docs/plugin-system.md and docs/operations.md.

CREATE TABLE IF NOT EXISTS plugin_kv (
    plugin     VARCHAR(64)  NOT NULL,
    k          VARCHAR(255) NOT NULL,
    v          MEDIUMTEXT   NOT NULL,
    expires_at DATETIME(6)  NULL,
    updated_at DATETIME(6)  NOT NULL,
    PRIMARY KEY (plugin, k),
    KEY idx_plugin_kv_expires (expires_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
