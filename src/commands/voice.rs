use serenity::framework::standard::{macros::command, Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::sync::Arc;
use songbird::Call;
use songbird::tracks::LoopState;
use songbird::input::Input;
use songbird::input::restartable::Restartable;

use tracing::error;

#[command]
#[only_in(guilds)]
pub async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let channel_id = guild
        .voice_states.get(&msg.author.id)
        .and_then(|voice_state| voice_state.channel_id);

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            msg.reply(&ctx.http, "You are not connected to a voice channel!").await.unwrap();
            return Ok(());
        }
    };

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();

    let _handler = manager.join(guild_id, connect_to).await;
    Ok(())
}

#[command]
#[aliases(dis, disconnect)]
#[only_in(guilds)]
pub async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            msg.channel_id.say(&ctx.http, format!("Failed: {:?}", e)).await.unwrap();
        }
    }
    else {
        msg.channel_id.say(&ctx.http, "The bot is not connected to a voice channel!").await.unwrap();
    }

    Ok(())
}

#[command]
#[aliases(p)]
#[only_in(guilds)]
pub async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let _ = match args.single::<String>() {
        Ok(r) => r,
        Err(_) => {
            msg.channel_id.say(&ctx.http, "No url or query provided!").await.unwrap();
            return Ok(());
        }
    };
    args.restore();
    let url = &args.rest();
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();

    let handler_lock = match manager.get(guild_id) {
        Some(a) => a,
        None => call_join(ctx, msg, args.clone()).await,
    };
    let mut handler = handler_lock.lock().await;

    let source;

    if url.starts_with("http") {
        source = match songbird::ytdl(&url).await {
            Ok(source) => source,
            Err(why) => {
                error!("Error starting source : {:?}", why);
                msg.channel_id.say(&ctx.http, "Error sourcing ffmpeg").await.unwrap();
                return Ok(());
            },
        };
    }
    else {
        source = match songbird::input::ytdl_search(&url).await {
            Ok(source) => source,
            Err(why) => {
                error!("Error starting source : {:?}", why);
                msg.channel_id.say(&ctx.http, "Error sourcing ffmpeg").await.unwrap();
                return Ok(());
            },
        };
    }

    let title = source.metadata.title.clone().unwrap();
    handler.enqueue_source(source);
    let q = handler.queue();
    if q.len() == 1 {
        msg.channel_id.say(&ctx.http, format!("Started playing `{}`", title)).await.unwrap();
    }
    else {
        msg.channel_id.say(&ctx.http, format!("Enqueued `{}`! It is currently in the position #{}", title, q.len()-1)).await.unwrap();
    }
    Ok(())
}

#[command]
#[aliases(s)]
#[only_in(guilds)]
pub async fn skip(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;
    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();
    let handler_lock = match manager.get(guild_id) {
        Some(a) => a,
        None => return Ok(()),
    };
    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    let old = queue.current().unwrap();
    queue.skip().unwrap();
    msg.channel_id.say(&ctx.http, format!("Skipped track `{}`!", old.metadata().title.clone().unwrap())).await.unwrap();
    Ok(())
}

#[command]
#[only_in(guilds)]
pub async fn pause(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;
    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();
    let handler_lock = match manager.get(guild_id) {
        Some(a) => a,
        None => return Ok(()),
    };
    let handler = handler_lock.lock().await;
    handler.queue().pause().unwrap();
    msg.channel_id.say(&ctx.http, "Playback paused!").await.unwrap();
    Ok(())
}

#[command]
#[only_in(guilds)]
pub async fn resume(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;
    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();
    let handler_lock = match manager.get(guild_id) {
        Some(a) => a,
        None => return Ok(()),
    };
    let handler = handler_lock.lock().await;
    handler.queue().resume().unwrap();
    msg.channel_id.say(&ctx.http, "Playback resumed!").await.unwrap();
    Ok(())
}

#[command]
#[aliases(q)]
#[only_in(guilds)]
pub async fn queue(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;
    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();
    let handler_lock = match manager.get(guild_id) {
        Some(a) => a,
        None => return Ok(()),
    };
    let handler = handler_lock.lock().await;
    let queue = handler.queue().current_queue();
    msg.channel_id.send_message(&ctx.http, |m| {
        m.embed(|e| {
            e.title(format!("Track queue ({} tracks)", queue.len()-1));
            e.color(serenity::utils::Color::PURPLE);
            for (position, track) in queue.iter().enumerate() {
                if position != 0 {
                    let metadata = track.metadata();
                    let duration = track.metadata().duration.unwrap();
                    let mut secs = duration.as_secs();
                    let mins: u64 = secs/60;
                    secs = secs-mins*60;
                    e.field(format!("#{}: {} ({}m{}s)", position, metadata.title.clone().unwrap(), mins, secs),
                        format!("[link to YouTube-video]({})", metadata.source_url.clone().unwrap()), false);
                }
            }
            e
        })
    }).await.unwrap();
    Ok(())
}

#[command]
#[only_in(guilds)]
pub async fn clear(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;
    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();
    let handler_lock = match manager.get(guild_id) {
        Some(a) => a,
        None => return Ok(()),
    };
    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    let amount = queue.len()-1;
    queue.modify_queue(|q| {
        for i in 1..q.len() {
            q[i].stop().unwrap();
            q.remove(i);
        }
    });
    if amount == 1 {
        msg.channel_id.say(&ctx.http, "Removed `1` track from the queue").await.unwrap();
    }
    else {
        msg.channel_id.say(&ctx.http, format!("Removed `{}` tracks from the queue", amount)).await.unwrap();
    }
    Ok(())
}

#[command]
#[only_in(guilds)]
pub async fn mombasa(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;
    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();
    let handler_a = match manager.get(guild_id) {
        Some(a) => a,
        None => {
            let channel_id = guild
                .voice_states.get(&msg.author.id)
                .and_then(|voice_state| voice_state.channel_id);

            let connect_to = match channel_id {
                Some(channel) => channel,
                None => {
                    msg.reply(&ctx.http, "You are not connected to a voice channel!").await.unwrap();
                    return Ok(());
                }
            };
            manager.join(guild_id, connect_to).await.1.unwrap();
            manager.get(guild_id).unwrap()
        },
    };
    let mut handler = handler_a.lock().await;
    clear(ctx, msg, args.clone());
    let restartable = Restartable::ytdl_search("taiska mombasa", true).await.unwrap();
    let input = Input::from(restartable);
    let (track, handel) = songbird::tracks::create_player(input);
    handler.enqueue(track);
    handel.enable_loop().unwrap();
    msg.channel_id.say(&ctx.http, "Jäi Mombasaan, vain päivä elämää! Ja elämään, nyt Mombasa vain jää! :notes:").await.unwrap();
    Ok(())
}

#[command]
#[aliases(loop)]
#[only_in(guilds)]
pub async fn toggle_loop(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;
    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialization.").clone();
    let handler_lock = match manager.get(guild_id) {
        Some(a) => a,
        None => return Ok(()),
    };
    let handler = handler_lock.lock().await;
    let track = match handler.queue().current() {
        Some(t) => t,
        None => {
            msg.channel_id.say(&ctx.http, "Nothing is playing right now!").await.unwrap();
            return Ok(());
        },
    };
    let state = track.get_info().await.unwrap();
    if state.loops == LoopState::default() {
        track.enable_loop().unwrap();
        msg.channel_id.say(&ctx.http, "Looping enabled!").await.unwrap();
    }
    else {
        track.disable_loop().unwrap();
        msg.channel_id.say(&ctx.http, "Looping disabled!").await.unwrap();
    }
    Ok(())
}

async fn call_join(ctx: &Context, msg: &Message, args: Args) -> Arc<Mutex<Call>> {
    join(ctx, msg, args).await.unwrap();
    return songbird::get(ctx).await.unwrap().get(msg.guild(&ctx.cache).await.unwrap().id).unwrap()
}
