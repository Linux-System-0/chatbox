-- Editable profiles and per-contact avatars.
--
-- `avatar_url` stores either an absolute http(s) URL or a data:image/... URL.
-- The same representation is used for the private per-contact remark avatar.

ALTER TABLE users
    ADD COLUMN avatar_url TEXT NULL AFTER display_name;

ALTER TABLE contacts
    ADD COLUMN remark_avatar_url TEXT NULL AFTER remark;
