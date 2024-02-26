-- Your SQL goes here

CREATE TABLE admin_bot_spam_channel (
    id INT NOT NULL GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    channel_id BIGINT NOT NULL,
    guild_id BIGINT UNIQUE NOT NULL, 
    CONSTRAINT fk_guild 
        FOREIGN KEY (guild_id)
            REFERENCES guilds(guild_id)
            ON DELETE CASCADE
)