-- Your SQL goes here

CREATE TABLE admit_bot_spam_channel (
    channel_id INT NOT NULL,
    guild_id INT UNIQUE PRIMARY KEY NOT NULL, 
    CONSTRAINT fk_guild 
        FOREIGN KEY (guild_id)
            REFERENCES guilds(guild_id)
            ON DELETE CASCADE
)