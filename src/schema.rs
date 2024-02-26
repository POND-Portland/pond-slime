// @generated automatically by Diesel CLI.

diesel::table! {
    admin_bot_spam_channel (guild_id) {
        channel_id -> Int8,
        guild_id -> Int8,
    }
}
