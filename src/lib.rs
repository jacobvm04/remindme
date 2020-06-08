use serenity::framework::standard::Args;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH, SystemTimeError};
use redis;
use redis::Commands;

#[derive(Serialize, Deserialize)]
pub struct Reminder {
    created_at: u64,
    pub author: u64,
    pub message: String,
}

impl Reminder {
    pub fn create_reminder(time_offset: u64, author: u64, message: String) -> Result<(u64, Reminder), SystemTimeError> {
        let created_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let reminder_timestamp = created_at + time_offset;

        Ok((reminder_timestamp, Reminder { created_at, author, message }))
    }

    pub fn create_serialized_reminder(time_offset: u64, author: u64, message: String) -> Result<(u64, String), Box<dyn std::error::Error>> {
        let (timestamp, reminder) = Reminder::create_reminder(time_offset, author, message)?;
        let serialized_reminder = serde_json::to_string(&reminder)?;

        Ok((timestamp, serialized_reminder))
    }
}

pub enum InvalidReminderArguments {
    InvalidTimeUnit(String),    
}

impl std::fmt::Display for InvalidReminderArguments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidReminderArguments::InvalidTimeUnit(invalid_unit) => write!(f, "{} is an invalid time unit", invalid_unit),
        }
    }
}

impl std::fmt::Debug for InvalidReminderArguments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidReminderArguments::InvalidTimeUnit(invalid_unit) => write!(f, "Use of invalid time unit: {} is an invalid", invalid_unit),
        }    
    }
}

impl std::error::Error for InvalidReminderArguments {}

pub struct ReminderArguments(pub u64, pub String, pub String, pub u64);

pub fn parse_reminder_arguments(args: &mut Args) -> Result<ReminderArguments, Box<dyn std::error::Error>> {
    let time_offset = args.single::<u64>()?;
    let time_unit = args.single::<String>()?;
    let message = args.rest().to_owned();

    let time_seconds_offset = match time_unit.as_str() {
        "seconds" => time_offset,
        "second" => time_offset,
        "minute" => time_offset * 60,
        "minutes" => time_offset * 60,
        "hours" => time_offset * 60 * 60,
        "hour" => time_offset * 60 * 60,
        "days" => time_offset * 24 * 60 * 60,
        "day" => time_offset * 24 * 60 * 60,
        "weeks" => time_offset * 7 * 24 * 60 * 60,
        "week" => time_offset * 7 * 24 * 60 * 60,
        _ => { return Err(Box::new(InvalidReminderArguments::InvalidTimeUnit(time_unit))); },
    };

    Ok(ReminderArguments(time_seconds_offset, message, time_unit, time_offset))
}

pub struct RedisPool {
    connection: redis::Connection,
}

impl RedisPool {
    pub fn new(connection_info: &str) -> RedisPool {
        let client = redis::Client::open(connection_info).expect("Error connecting to redis for schedule processing");
        let connection = client.get_connection().expect("Error getting connection to redis for schedule processing");
        RedisPool { connection }
    }

    pub fn get_connection(&mut self) -> &mut redis::Connection {
        &mut self.connection
    }
}

pub struct RedisMessageScheduler {
    key: String,
    pool: RedisPool,
}

impl RedisMessageScheduler {
    pub fn new(key: String, pool: RedisPool) -> RedisMessageScheduler {
        RedisMessageScheduler { key, pool }
    }

    pub fn add_message<M: redis::ToRedisArgs>(&mut self, message: M, timestamp: u64) -> Result<(), redis::RedisError>{
        self.pool.get_connection().zadd(&self.key, message, timestamp)?;
        Ok(())
    }
}
