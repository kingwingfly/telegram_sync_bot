use anyhow::Result;
use log::info;
use teloxide::{
    Bot,
    prelude::Requester as _,
    requests::HasPayload as _,
    types::{ChatId, MessageId, ReactionType},
};

pub(super) async fn set_emoji(
    bot: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    emoji: impl AsRef<str>,
) -> Result<()> {
    let mut req = bot.set_message_reaction(chat_id, msg_id);
    req.payload_mut().reaction = Some(vec![ReactionType::Emoji {
        emoji: emoji.as_ref().to_string(),
    }]);
    req.await?;
    Ok(())
}

pub(super) async fn pin_msg(bot: &Bot, chat_id: ChatId, msg_id: MessageId) -> Result<()> {
    bot.pin_chat_message(chat_id, msg_id).await?;
    info!("Pinned message: {}", msg_id.0);
    Ok(())
}

pub(super) async fn unpin_msg(bot: &Bot, chat_id: ChatId, msg_id: MessageId) -> Result<()> {
    let mut unpin = bot.unpin_chat_message(chat_id);
    unpin.message_id = Some(msg_id);
    unpin.await?;
    info!("Unpinned message: {}", msg_id.0);
    Ok(())
}
