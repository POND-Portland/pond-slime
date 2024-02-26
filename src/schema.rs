// @generated automatically by Diesel CLI.

diesel::table! {
    admit_bot_spam_channel (guild_id) {
        channel_id -> Int4,
        guild_id -> Int4,
    }
}

diesel::table! {
    guilds (guild_id) {
        guild_id -> Int4,
    }
}

diesel::joinable!(admit_bot_spam_channel -> guilds (guild_id));

diesel::allow_tables_to_appear_in_same_query!(
    admit_bot_spam_channel,
    guilds,
);
