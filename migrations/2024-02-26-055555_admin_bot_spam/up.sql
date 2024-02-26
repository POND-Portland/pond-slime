-- Your SQL goes here

CREATE TABLE admin_bot_spam_channel (
    channel_id BIGINT NOT NULL,
    guild_id BIGINT UNIQUE NOT NULL PRIMARY KEY, 
    CONSTRAINT fk_guild 
        FOREIGN KEY (guild_id)
            REFERENCES guilds(guild_id)
            ON DELETE CASCADE
)