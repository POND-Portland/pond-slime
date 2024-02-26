// @generated automatically by Diesel CLI.

diesel::table! {
    admin_bot_spam_channel (guild_id) {
        channel_id -> Int8,
        guild_id -> Int8,
    }
}

diesel::table! {
    guilds (guild_id) {
        guild_id -> Int8,
    }
}

diesel::joinable!(admin_bot_spam_channel -> guilds (guild_id));

diesel::allow_tables_to_appear_in_same_query!(
    admin_bot_spam_channel,
    guilds,
);
