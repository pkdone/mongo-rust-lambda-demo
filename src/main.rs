use bson::DateTime;
use lambda_runtime::{handler_fn, Context, Error as LambdaError};
use lazy_static::lazy_static;
use log::{debug, error, info};
use mongodb::{Client, Collection};
use once_cell::sync::OnceCell;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::borrow::Cow;
use std::env;
use std::error::Error;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

// Constants
const MONGODB_URL_VAR: &str = "MONGODB_URL";
const DBNAME: &str = "test";
const COLLNAME: &str = "lambdalogs";

// Statics
static MONGODB_CLIENT: OnceCell<Client> = OnceCell::new();
static INVOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);

// To capture data for insertion into DB
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DBLogRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocated_memory: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_deadline_millis: Option<u64>,
}

// Main bootstrap function to setup the lambda function
//
#[tokio::main]
async fn main() -> Result<(), LambdaError> {
    env_logger::init();
    let mongodb_url = get_mongodb_url_from_env_var()?;
    create_mongodb_client(&mongodb_url).await?;
    let func = handler_fn(handler);
    lambda_runtime::run(func).await?;
    info!("Lambda initiated to use MongoDB deployment: '{}'", redact_mongodb_url(&mongodb_url));
    Ok(())
}

// Handler function executed each time the lambda function is invoked
//
async fn handler(event: Value, context: Context) -> Result<Value, LambdaError> {
    let message = event["message"].as_str().unwrap_or("Missing input payload message");
    let result =
        process_work(message, &context.request_id, context.env_config.memory, context.deadline)
            .await;

    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            error!("Internal error occurred in the lambda function: {}", e);
            Err("An internal error occurred".into())
        }
    }
}

// Core execution work of the lambda function, separated from handler wrapper function to be easily
// invocable via integration tests at the base of this source code file
//
async fn process_work(
    message: &str, request_id: &str, memory: i32, deadline: u64,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    let mongodb_url = get_mongodb_url_from_env_var()?;
    info!(
        "Lambda function executing request against MongoDB deployment: '{}'",
        redact_mongodb_url(&mongodb_url)
    );
    let mongodb_client = get_mongodb_client()?;
    let invocation_count = increment_count_and_fetch();
    let cpu_cores = run_os_cmd("nproc", &["--all"])?.parse::<i32>()?;
    let coll = mongodb_client.database(DBNAME).collection(COLLNAME);
    db_insert_record(&coll, invocation_count, message, request_id, cpu_cores, memory, deadline)
        .await?;
    Ok(json!(
        {
            "mongodb_url": mongodb_url,
            "invocation_count": invocation_count,
            "action": "Log record inserted into DB",
            "message_received": message,
        }
    ))
}

// Inserts some log data as a new document in a MongoDB database collection
//
async fn db_insert_record(
    coll: &Collection<DBLogRecord>, invocation_count: usize, message: &str, request_id: &str,
    cpu_cores: i32, memory: i32, deadline: u64,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let record = DBLogRecord {
        timestamp: Some(DateTime::now()),
        invocation_count: Some(invocation_count),
        message: Some(message.to_string()),
        aws_request_id: Some(request_id.to_string()),
        cpu_cores: Some(cpu_cores),
        allocated_memory: Some(memory),
        execution_deadline_millis: Some(deadline),
    };
    coll.insert_one(record, None).await?;
    Ok(())
}

// Increment the atomic number counter and return its new value
//
fn increment_count_and_fetch() -> usize {
    INVOCATION_COUNT.fetch_add(1, Ordering::SeqCst) + 1
}

// Get the already cached mongodb client
//
fn get_mongodb_client() -> Result<&'static Client, Box<dyn Error + Send + Sync>> {
    MONGODB_CLIENT.get().ok_or_else(|| "Missing MongoDB client as static reference".into())
}

// Cache a new mongodb client
//
async fn create_mongodb_client(mongodb_url: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client_result = Client::with_uri_str(mongodb_url).await;
    debug!("Client connection: {:#?}", client_result);

    match client_result {
        Ok(client) => match MONGODB_CLIENT.set(client) {
            Ok(()) => Ok(()),
            Err(_) => {
                const ERRMSG: &str = "Error saving MongoDB client in a static reference";
                error!("{}", ERRMSG);
                Err(ERRMSG.into())
            }
        },
        Err(e) => {
            error!(
                "Error trying to get a MongoDB connection to the URL '{}'. Error detail: {}",
                redact_mongodb_url(mongodb_url),
                e
            );
            Err(Box::new(e))
        }
    }
}

// Get the URL of the MongoDB database to connect to, from an environment variable
//
fn get_mongodb_url_from_env_var() -> Result<String, Box<dyn Error + Send + Sync>> {
    match env::var(MONGODB_URL_VAR) {
        Ok(val) => Ok(val),
        Err(e) => {
            error!(
                "Unable to run lambda function because env var not located: '{}' - err: {}",
                MONGODB_URL_VAR, e
            );
            Err("Internal error - lambda function didn't initialize properly".into())
        }
    }
}

// Obfuscate the real username and password in a Mongodb URL with hardcoded dummy values, returning
// the redacted URL
//
fn redact_mongodb_url(mongodb_url: &str) -> Cow<str> {
    lazy_static! {
        static ref MONGODB_URL_PATTERN: Regex =
            Regex::new(r"(?P<prefix>mongodb(\+srv)?://)(.+):(.+)(?P<suffix>@.+)")
                .expect("Expected constructed regex");
    }

    MONGODB_URL_PATTERN.replace(mongodb_url, "${prefix}REDACTED:REDACTED$suffix")
}

// Run a command on the host OS returning the command's output
//
pub fn run_os_cmd(cmd: &str, args: &[&str]) -> Result<String, Box<dyn Error + Send + Sync>> {
    let cmd_result = Command::new(cmd).args(args).output()?;
    Ok(String::from_utf8_lossy(&cmd_result.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_test_url1() {
        let before = "mongodb+srv://main_user:mypwd@mycluster.aa.mongodb.net/";
        let after = redact_mongodb_url(before);
        assert_eq!(after, "mongodb+srv://REDACTED:REDACTED@mycluster.aa.mongodb.net/");
    }

    #[test]
    fn unit_test_url2() {
        let before = "mongodb://main_user:mypwd@mycluster.a282e.mongodb.net";
        let after = redact_mongodb_url(before);
        assert_eq!(after, "mongodb://REDACTED:REDACTED@mycluster.a282e.mongodb.net");
    }

    #[test]
    fn unit_test_url3() {
        let before = "mongodb://main_user:mypwd@mycluster.aa.mongodb.net?ww=yy";
        let after = redact_mongodb_url(before);
        assert_eq!(after, "mongodb://REDACTED:REDACTED@mycluster.aa.mongodb.net?ww=yy");
    }

    #[test]
    fn unit_test_url4() {
        let before = "mongodb+srv://main_user:mypwd@mycluster.aa.mongodb.net/test?ww=yy";
        let after = redact_mongodb_url(before);
        assert_eq!(after, "mongodb+srv://REDACTED:REDACTED@mycluster.aa.mongodb.net/test?ww=yy");
    }

    #[test]
    fn unit_test_url5() {
        let before = "mongodb://localhost:27017";
        let after = redact_mongodb_url(before);
        assert_eq!(after, "mongodb://localhost:27017");
    }

    #[test]
    fn unit_test_url6() {
        let before = "mongodb://aa:bb@localhost:27017";
        let after = redact_mongodb_url(before);
        assert_eq!(after, "mongodb://REDACTED:REDACTED@localhost:27017");
    }

    #[test]
    fn unit_test_url7() {
        let before = "mongodb://machine1:27017;machine2:27017";
        let after = redact_mongodb_url(before);
        assert_eq!(after, "mongodb://machine1:27017;machine2:27017");
    }

    #[test]
    fn unit_test_url8() {
        let before = "mongodb://aa:bb@machine1:27017;machine2:27017/?x=y";
        let after = redact_mongodb_url(before);
        assert_eq!(after, "mongodb://REDACTED:REDACTED@machine1:27017;machine2:27017/?x=y");
    }

    #[test]
    #[ignore]
    fn integration_test_execute_full_flow() -> Result<(), Box<dyn Error + Send + Sync>> {
        env_logger::init();
        let mongodb_url = get_mongodb_url_from_env_var()?;
        let rt = tokio::runtime::Runtime::new().expect("Expected the Tokio runtime");

        rt.block_on(async {
            create_mongodb_client(&mongodb_url).await.expect("Expected MongoDB client");
            process_work("Hello from integration test", "integration_test_execute_full_flow", 0, 0)
                .await
                .map(|_| ())
        })
    }
}
