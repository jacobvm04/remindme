use std::time::{SystemTime, UNIX_EPOCH};
use std::thread;
use std::env;
use std::time::Duration;
use redis;
use std::sync::Arc;
use redis::Commands;
use flexi_logger::{Logger, opt_format, Duplicate};
use serenity::client::Client;
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use serenity::model::gateway::Ready;
use serenity::prelude::{EventHandler, Context};
use serenity::framework::standard::{
    Args,
    StandardFramework,
    CommandResult,
    macros::{
        command,
        group
    }
};

#[macro_use]
extern crate log;

#[group]
#[commands(reminder)]
struct General;

struct Handler;
impl EventHandler for Handler {
    fn ready(&self, _: Context, ready: Ready) {
        info!("{} has connected!", ready.user.name);
    }
}

fn main() {
    let mut client = Client::new(&env::var("DISCORD_TOKEN").expect("Discord token env variable not set"), Handler)
        .expect("Error creating client");

    client.with_framework(StandardFramework::new()
        .configure(|c| c.prefix("!"))
        .group(&GENERAL_GROUP));

    let http = Arc::clone(&Arc::clone(&client.cache_and_http).http);

    Logger::with_str("info")
        .log_to_file()
        .directory("logs")
        .format(opt_format)
        .duplicate_to_stderr(Duplicate::All)
        .start()
        .expect("Error starting logger");

    thread::spawn(move || {
        let mut pool = remindme::RedisPool::new("redis://127.0.0.1");

        loop {
            let redis_conn = pool.get_connection();
            let item: Option<(String, u64)> = redis_conn.zrange_withscores("reminder_queue", 0, 0).unwrap_or(None);
            if let None = item {
                thread::sleep(Duration::from_millis(100));
                continue;
            }

            let (serialized_reminder, reminder_timestamp) = item.unwrap();

            let current_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("Error getting current timestamp").as_secs();
            if reminder_timestamp > current_timestamp {
                thread::sleep(Duration::from_millis(100));
                continue;
            }

            let reminder: remindme::Reminder = serde_json::from_str(&serialized_reminder).expect("Error parsing reminder json");

            let _: () = redis_conn.zrem("reminder_queue", serialized_reminder).unwrap_or(());

            let dm = UserId(reminder.author).create_dm_channel(&http).expect("Error creating dm");
            dm.say(&http, format!("Reminder: {}", reminder.message)).expect("Error sending dm reminder");
            thread::sleep(Duration::from_millis(100));
        }
    });

    if let Err(e) = client.start_autosharded() {
        eprintln!("Error while running client: {}", e);
    }
}

#[command]
fn reminder(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
    let remindme::ReminderArguments(time_offset, message, time_unit, raw_time_offset) = match remindme::parse_reminder_arguments(&mut args) {
        Ok(ret) => ret,
        Err(err) => { 
            args.restore();
            info!("The following arguments were deemed invalid: {}, by the following error: {}", args.rest(), err);
            msg.channel_id.say(ctx, "Please make sure to use the command format\n`!reminder [time_amount] [time_unit] [reminder message]`\nYour options for time_unit are second, seconds, minute, minutes, hour, hours, day, days, week, or weeks.")?;
            return Ok(()); 
        } 
    };  

    let (reminder_timestamp, serialized_reminder) = match remindme::Reminder::create_serialized_reminder(time_offset, msg.author.id.0, message) {
        Ok(ret) => ret,
        Err(err) => {
            error!("The following error occured while attempting to serialize a reminder: {}", err);
            msg.channel_id.say(ctx, "An error occured while preparing your reminder. Please try again later.")?;
            return Ok(());
        }
    };

    let pool = remindme::RedisPool::new("redis://127.0.0.1");
    let mut message_scheduler = remindme::RedisMessageScheduler::new("reminder_queue".to_owned(), pool);

    match message_scheduler.add_message(serialized_reminder, reminder_timestamp) {
        Ok(()) => (),
        Err(err) => {
            error!("The following error occured while attempting to add the serialized reminder and timestamp to the message scheduler: {}", err);
            msg.channel_id.say(ctx, "An error occured while scheduling your reminder. Please try again later.")?;
            return Ok(());
        }
    };

    msg.channel_id.say(ctx, format!("You will be remined {} {} from now", raw_time_offset, &time_unit))?;
    
    Ok(())
}
