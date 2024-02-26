// @generated automatically by Diesel CLI.

diesel::table! {
    admin_bot_spam_channel (id) {
        id -> Int4,
        channel_id -> Int8,
        guild_id -> Int8,
    }
}

diesel::table! {
    guilds (id) {
        id -> Int4,
        guild_id -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    admin_bot_spam_channel,
    guilds,
);
