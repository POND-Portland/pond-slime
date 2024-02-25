use anyhow::anyhow;
use serenity::Error as SerenityError;
use shuttle_secrets::SecretStore;
use thiserror::Error;
use tracing::error;

use poise::serenity_prelude::*;

#[derive(Clone)]
struct Data {
    pool: sqlx::PgPool,
}

#[derive(Error, Debug)]
enum SlimeError {
    #[error("an occur occurred within Serenity: {0}")]
    SerenityError(#[from] SerenityError),
}
type Context<'a> = poise::Context<'a, Data, SlimeError>;

#[poise::command(slash_command)]
async fn hello(ctx: Context<'_>) -> Result<(), SlimeError> {
    ctx.say("world!").await?;
    Ok(())
}

#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
    #[shuttle_shared_db::Postgres] pool: sqlx::PgPool,
) -> shuttle_serenity::ShuttleSerenity {
    // Get the discord token set in `Secrets.toml`
    let token = if let Some(token) = secret_store.get("DISCORD_TOKEN") {
        token
    } else {
        return Err(anyhow!("'DISCORD_TOKEN' was not found").into());
    };

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_SCHEDULED_EVENTS
        | GatewayIntents::DIRECT_MESSAGES;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![hello()],
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
