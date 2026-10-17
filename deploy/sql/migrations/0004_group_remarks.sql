-- Private per-member aliases for group conversations.
--
-- A group member can set their own remark (a personal display name for the
-- group) without changing the group name seen by other members. Group-wide
-- metadata still lives in `chat_groups.name`.

ALTER TABLE group_members
    ADD COLUMN remark VARCHAR(64) NULL AFTER role;
