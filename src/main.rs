use std::fmt::Write;

use anyhow::anyhow;
use chrono::{DateTime, Duration, Utc};
use diesel::{prelude::*, result::Error as DieselError, upsert, Queryable};
use diesel_async::{
    pooled_connection::{
        deadpool::{Pool, PoolError},
        AsyncDieselConnectionManager,
    },
    AsyncPgConnection, RunQueryDsl,
};
use diesel_async_migrations::EmbeddedMigrations;
use poise::{serenity_prelude::*, CreateReply};
use serenity::{
    futures::{future, StreamExt, TryStreamExt},
    Error as SerenityError,
};
use shuttle_secrets::SecretStore;
use thiserror::Error;
use tokio::time::Instant;
use tracing::error;

mod schema;

const METER_LIMIT: usize = 500;

static MIGRATIONS: EmbeddedMigrations = diesel_async_migrations::embed_migrations!();

#[derive(Clone)]
struct Data {
    pool: Pool<AsyncPgConnection>,
}

#[derive(Queryable, Selectable, Insertable, Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[diesel(table_name = schema::guilds)]
struct Guild {
    guild_id: i64,
}

impl From<GuildId> for Guild {
    fn from(value: GuildId) -> Self {
        Self {
            guild_id: value.get() as i64,
        }
    }
}

impl From<Guild> for GuildId {
    fn from(value: Guild) -> Self {
        GuildId::from(value.guild_id as u64)
    }
}

#[derive(Error, Debug)]
enum SlimeError {
    #[error("an error occurred within Serenity: {0}")]
    Serenity(#[from] SerenityError),
    #[error("an error occurred within sqlx: {0}")]
    DatabasePool(#[from] PoolError),
    #[error("an error occurred from a diesel query: {0}")]
    Diesel(#[from] DieselError),
}
type Context<'a> = poise::Context<'a, Data, SlimeError>;

fn make_uuid_buttons(yes_uuid: &str, no_uuid: &str, disabled: bool) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new(yes_uuid)
            .label("yes")
            .style(ButtonStyle::Danger)
            .disabled(disabled),
        CreateButton::new(no_uuid)
            .label("no")
            .style(ButtonStyle::Secondary)
            .disabled(disabled),
    ])
}

async fn messages_before(
    ctx: Context<'_>,
    before: DateTime<Utc>,
    channel: ChannelId,
) -> Result<Vec<Message>, SlimeError> {
    Ok(channel
        .messages_iter(ctx)
        .skip_while(|v| {
            future::ready(
                v.as_ref()
                    .map(|msg| msg.timestamp.to_utc() >= before)
                    .unwrap_or(false),
            )
        })
        .try_collect()
        .await?)
}

async fn bulk_delete(
    ctx: Context<'_>,
    messages: &[Message],
    dry_run: bool,
) -> Result<(), SlimeError> {
    debug_assert!(
        messages[messages.len() - 1].timestamp.to_utc() > Utc::now() - Duration::weeks(2)
    );

    let mut start_time = Instant::now();

    let mut count = 0;
    for chunk in messages.chunks(100) {
        if !dry_run {
            ctx.channel_id().delete_messages(ctx, chunk).await?;
        }
        count += 1;

        if count >= METER_LIMIT {
            tokio::time::sleep_until(start_time + tokio::time::Duration::from_secs(60)).await;
            count = 0;
            start_time = Instant::now();
        }
    }

    Ok(())
}

async fn slow_bulk_delete(
    ctx: Context<'_>,
    messages: &[Message],
    dry_run: bool,
) -> Result<(), SlimeError> {
    let mut count = 0;
    let mut start_time = Instant::now();

    for message in messages {
        if !dry_run {
            ctx.channel_id().delete_message(ctx, message).await?;
        }
        count += 1;

        if count >= METER_LIMIT {
            tokio::time::sleep_until(start_time + tokio::time::Duration::from_secs(60)).await;
            count = 0;
            start_time = Instant::now();
        }
    }

    Ok(())
}

/// Bulk deletes messages from the supplied channel. Warning: This can take a very long time.
#[poise::command(
    slash_command,
    category = "delete",
    guild_only = true,
    default_member_permissions = "ADMINISTRATOR"
)]
async fn purge_old(
    ctx: Context<'_>,
    #[description = "the channel to purge from"] channel: Channel,
    #[description = "whether to actually run the command or merely show progress as if it were running"]
    dry_run: Option<bool>,
) -> Result<(), SlimeError> {
    let before = Utc::now() - chrono::Duration::days(7);
    let dry_run = dry_run.unwrap_or(false) || cfg!(debug);

    ctx.defer().await?;

    let messages = messages_before(ctx, before, channel.id()).await?;

    let bulk_cutoff = Utc::now() - (chrono::Duration::days(13) + chrono::Duration::hours(12));

    let mut content = String::from("I'll help you purge old messages!\n\n");
    if messages.is_empty() {
        return Ok(());
    }

    let (slow_index, mut minutes) = if let Some((idx, msg)) = messages
        .iter()
        .enumerate()
        .find(|(_, msg)| msg.timestamp.to_utc() < bulk_cutoff)
    {
        let old_message_count = messages.len() - idx;
        let minutes_to_delete = (old_message_count as f64) / (METER_LIMIT as f64);

        write!(
            &mut content,
            "This deletion has {old_message_count} messages beyond the bulk cutoff window!\n\
            At a rate of {METER_LIMIT} messages per minute, deleting these will take approximately {minutes_to_delete:.2} minutes.\n\
            The first message in this set is <{}>, and the last is <{}>.\n\n",
            messages[messages.len()-1].link(),
            msg.link(),
        )
        .unwrap();

        (Some(idx), minutes_to_delete)
    } else {
        (None, 0.)
    };

    let bulk_count = slow_index.unwrap_or(0);
    minutes += if bulk_count > 0 {
        let msgs_per_min = METER_LIMIT * 100;
        let minutes_to_delete = (bulk_count as f64) / (msgs_per_min as f64);
        write!(
            &mut content,
            "This deletion has {bulk_count} messages that can be *bulk* deleted!\n\
            At a rate of {msgs_per_min} messages per minute, deleting these will take approximately {minutes_to_delete:.2} minutes.\n\
            The first message in this set is <{}>, and the last is <{}>.\n\n",
            messages[bulk_count-1].link(), messages[0].link(),
        ).unwrap();

        minutes_to_delete
    } else {
        0.
    };

    write!(&mut content, "Overall, this will take {minutes:.2} minutes to complete, starting with the bulk messages. Continue?").unwrap();

    let id = ctx.id();
    let yes_uuid: String = format!("{id}-yes");
    let no_uuid: String = format!("{id}-no");

    let buttons = make_uuid_buttons(&yes_uuid, &no_uuid, false);

    let reply = CreateReply::default()
        .content(content)
        .components(vec![buttons]);
    ctx.send(reply).await?;

    if let Some(interactions) = ComponentInteractionCollector::new(ctx.serenity_context())
        .timeout(std::time::Duration::from_secs(120))
        .custom_ids(vec![yes_uuid.clone(), no_uuid.clone()])
        .await
    {
        let message = CreateInteractionResponseMessage::new()
            .components(vec![make_uuid_buttons("yes_disabled", "no_disabled", true)])
            .content(&interactions.message.content);

        let disable_buttons = CreateInteractionResponse::UpdateMessage(message);
        interactions
            .create_response(ctx, disable_buttons)
            .await
            .inspect_err(|e| error!("{}", e))?;

        let content = match &interactions.data.custom_id {
            id if id == &yes_uuid => "yes",
            id if id == &no_uuid => "no",
            _ => unreachable!(),
        };

        let followup = CreateInteractionResponseFollowup::new()
            .content(content)
            .ephemeral(true);
        interactions
            .create_followup(ctx, followup)
            .await
            .inspect_err(|e| error!("{}", e))?;
    }

    Ok(())
}

/// Sets the channel where bot spam (e.g. status updates) should happen. Default: current channel
#[poise::command(
    slash_command,
    category = "admin",
    guild_only = true,
    // default_member_permissions = "ADMINISTRATOR",
    ephemeral = true
)]
async fn admin_bot_spam_channel(
    ctx: Context<'_>,
    #[description = "the channel to purge from"] channel: Option<GuildChannel>,
) -> Result<(), SlimeError> {
    use diesel;
    use schema::{admin_bot_spam_channel, guilds};

    let channel = if let Some(channel) = channel {
        channel
    } else {
        ctx.guild_channel().await.unwrap()
    };
    let channel_id = channel.id;

    let mut conn = ctx.data().pool.get().await?;

    let guild: Guild = ctx.guild_id().unwrap().into();

    diesel::insert_into(guilds::table)
        .values(guild)
        .on_conflict_do_nothing()
        .execute(&mut conn)
        .await?;

    diesel::insert_into(admin_bot_spam_channel::table)
        .values((
            admin_bot_spam_channel::channel_id.eq(channel_id.get() as i64),
            admin_bot_spam_channel::guild_id.eq(guild.guild_id),
        ))
        .on_conflict(admin_bot_spam_channel::guild_id)
        .do_update()
        .set(
            admin_bot_spam_channel::guild_id.eq(upsert::excluded(admin_bot_spam_channel::guild_id)),
        )
        .execute(&mut conn)
        .await?;

    ctx.say(format!("Bot spam channel successfully set to {}", channel,))
        .await?;

    channel
        .send_message(
            ctx,
            CreateMessage::new().content(format!(
                "This channel is now my bot spam channel, as per {}'s orders!",
                ctx.author()
            )),
        )
        .await?;

    Ok(())
}

#[poise::command(slash_command)]
async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"] command: Option<String>,
) -> Result<(), SlimeError> {
    let config = poise::builtins::HelpConfiguration {
        extra_text_at_bottom: "\
Type /help command for more info on a command.
You can edit your message to the bot and the bot will edit its response.",
        ..Default::default()
    };

    poise::builtins::help(ctx, command.as_deref(), config).await?;
    Ok(())
}

#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
    #[shuttle_shared_db::Postgres] db_uri: String,
) -> shuttle_serenity::ShuttleSerenity {
    // Get the discord token set in `Secrets.toml`
    let token = if let Some(token) = secret_store.get("DISCORD_TOKEN") {
        token
    } else {
        return Err(anyhow!("'DISCORD_TOKEN' was not found").into());
    };

    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(db_uri);
    let pool = Pool::builder(config).build().unwrap();

    let mut conn = pool.get().await.expect("could not connect to shared DB");
    MIGRATIONS
        .run_pending_migrations(&mut conn)
        .await
        .expect("could not run migrations");

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_SCHEDULED_EVENTS
        | GatewayIntents::DIRECT_MESSAGES;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![purge_old(), admin_bot_spam_channel(), help()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data { pool })
            })
        })
        .build();

    let client = Client::builder(&token, intents)
        .framework(framework)
        .await
        .expect("Err creating client");

    Ok(client.into())
}
