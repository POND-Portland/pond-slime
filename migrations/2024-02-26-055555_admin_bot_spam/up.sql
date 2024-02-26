-- Your SQL goes here

CREATE TABLE admin_bot_spam_channel (
    channel_id BIGINT NOT NULL,
    guild_id BIGINT UNIQUE NOT NULL PRIMARY KEY
)