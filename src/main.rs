use anyhow::anyhow;
use serenity::Error as SerenityError;
use shuttle_secrets::SecretStore;
use thiserror::Error;
use tracing::error;

use poise::{serenity_prelude::*, CreateReply};
use uuid::Uuid;

#[derive(Clone)]
struct Data {
    _pool: sqlx::PgPool,
}

#[derive(Error, Debug)]
enum SlimeError {
    #[error("an occur occurred within Serenity: {0}")]
    SerenityError(#[from] SerenityError),
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

#[poise::command(slash_command)]
async fn purge_old(ctx: Context<'_>) -> Result<(), SlimeError> {
    let _channel = ctx.guild_channel().await.unwrap();

    let yes_uuid: String = Uuid::new_v4().into();
    let no_uuid: String = Uuid::new_v4().into();

    let buttons = make_uuid_buttons(&yes_uuid, &no_uuid, false);

    let reply = CreateReply::default()
        .content("The first message to be deleted is <foo>, the last is <bar> continue?")
        .components(vec![buttons])
        .ephemeral(true);

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
            commands: vec![purge_old()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data { _pool: pool })
            })
        })
        .build();

    let client = Client::builder(&token, intents)
        .framework(framework)
        .await
        .expect("Err creating client");

    Ok(client.into())
}
