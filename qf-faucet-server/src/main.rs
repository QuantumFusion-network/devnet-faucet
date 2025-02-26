use std::str::FromStr;

use subxt;
use subxt::{OnlineClient, PolkadotConfig};
use subxt_signer::bip39::Mnemonic;
use warp::{reject::Reject, Filter};
use subxt_signer::sr25519::dev;
use subxt_signer::sr25519;
use std::env;
use hex;
use clap::Parser;
use subxt::utils::{MultiAddress, AccountId32};
use rusqlite::{Connection, Result as SqlResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};


#[subxt::subxt(runtime_metadata_path = "metadata.scale", derive_for_all_types = "PartialEq, Eq")]
pub mod polkadot {}

const RPC_URL: &str = "wss://dev.qfnetwork.xyz/socket";
const FAUCET_AMOUNT: u128 = 20_000_000_000;

#[derive(Debug, Deserialize, Serialize)]
struct DripRequest {
    address: String,
}

#[derive(Debug)]
enum Errors {
    TransferError,
    StorageError,
    SomeError,
}


impl Reject for Errors {}

fn init_db() {
    let conn = get_db().expect("Init DB Error");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS transfers (
            id INTEGER PRIMARY KEY,
            address TEXT NOT NULL,
            amount INTEGER NOT NULL,
            timestamp INTEGER NOT NULL,
            tx_hash TEXT NOT NULL
        )",
        [],
    ).expect("Create Table Error");
    
    // Add index on address column
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_transfers_address ON transfers (address)",
        [],
    ).expect("Create Index Error");
    
    // Add index on timestamp column
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_transfers_timestamp ON transfers (timestamp)",
        [],
    ).expect("Create Index Error");
    
    // Add composite index on both address and timestamp for the specific query pattern
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_transfers_address_timestamp ON transfers (address, timestamp)",
        [],
    ).expect("Create Index Error");
}

fn get_db() -> SqlResult<Connection> {
    let conn = Connection::open("transfers.db")?;
    Ok(conn)
}

fn store_transfer(address: &str, amount: u64, tx_hash: &str, time: i64) -> SqlResult<()> {
    let conn = get_db()?;
    conn.execute(
        "INSERT INTO transfers (address, amount, timestamp, tx_hash) VALUES (?1, ?2, ?3, ?4)",
        (address, amount, time, tx_hash),
    )?;
    Ok(())
}

fn update_transfer(address: &str, time: i64, tx_hash: &str) -> SqlResult<()> {
    let conn = get_db()?;
    conn.execute(
        "UPDATE transfers SET tx_hash = ?2 WHERE address = ?1 AND timestamp = ?3",
        (address, tx_hash, time),
    )?;
    Ok(())
}

fn can_transfer(address: &str, timeout: Option<u64>) -> SqlResult<bool> {
    let conn = get_db()?;
    let now = Utc::now().timestamp() as u64;
    let two_hours_ago = now - (timeout.unwrap_or_else(|| 120) * 60);

    // Check if there are recent transfers
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM transfers 
        WHERE address = ?1 AND timestamp > ?2",
        [address, format!("{}", two_hours_ago).as_str()],
        |row| row.get(0),
    )?;
    
    // Delete all previous transfers from this address that are older than the timeout
    conn.execute(
        "DELETE FROM transfers WHERE address = ?1 AND timestamp < ?2",
        [address, format!("{}", two_hours_ago).as_str()],
    )?;

    Ok(count == 0)
}

async fn transfer_tokens(body: DripRequest, config: ServerConfig) -> Result<impl warp::Reply, warp::Rejection> {
    let can_transfer = can_transfer(body.address.as_str(), config.timeout)
        .map_err(|e| {
            println!("Error checking transfer: {:?}", e);
            warp::reject::custom(Errors::StorageError)
        })?;
    
    if !can_transfer {
        return Ok(warp::reply::json(&"You have already received tokens in the last 2 hours"));
    }

    let account_bytes = hex::decode(&body.address)
        .map_err(|e| {
            println!("Error: {:?}", e);
            warp::reject::custom(Errors::SomeError)
        })?;

    let account_array: [u8; 32] = account_bytes[..32].try_into()
        .map_err(|e| {
            println!("Error: {:?}", e);
            warp::reject::custom(Errors::SomeError)
        })?;
    let rpc_endpoint = config.rpc_url;
    let api = OnlineClient::<PolkadotConfig>::from_url(rpc_endpoint)
        .await
        .map_err(|e| {
            println!("Error: {:?}", e);
            warp::reject::custom(Errors::SomeError)
        })?;

    // Create a keypair from the provided address
    let mnemonic = env::var("MNEMONIC").expect("MNEMONIC is not set");
    let phrase = Mnemonic::from_str(&mnemonic).map_err(|e| {
        println!("Error: {:?}", e);
        warp::reject::custom(Errors::SomeError)
    })?;

    let from;
    if config.debug {
        println!("Transfer from Alice");
        from = dev::alice();
    } else {
        from = sr25519::Keypair::from_phrase(&phrase, None).map_err(|e| {
            println!("Error: {:?}", e);
            warp::reject::custom(Errors::SomeError)
        })?;
    }
    
    let dest = MultiAddress::Id(AccountId32(account_array));


    let transfer = polkadot::tx().balances().transfer_keep_alive(dest, FAUCET_AMOUNT.into());
    let now = Utc::now().timestamp();
    let _ = store_transfer(
        &body.address,
        (FAUCET_AMOUNT / 10_000_000_000) as u64,
        "",
    now)
        .map_err(|e| {
            println!("Error store the transfer: {:?}", e);
            warp::reject::custom(Errors::StorageError)
        })?;

    let events = api
        .tx()
        .sign_and_submit_then_watch_default(&transfer, &from)
        .await
        .map_err(|e| {
            println!("Error Submit transfer: {:?}", e);
            warp::reject::custom(Errors::TransferError)
        })?
        .wait_for_finalized_success()
        .await
        .map_err(|e| {
            println!("Error transfer not finalized: {:?}", e);
            warp::reject::custom(Errors::TransferError)
        })?;
    update_transfer(
        &body.address,
        now,
        &events.extrinsic_hash().to_string(),
    ).map_err(|e| {
        println!("Error update the transfer: {:?}", e);
        warp::reject::custom(Errors::StorageError)
    })?;

    Ok(warp::reply::json(&format!("Extrinsic submitted: {:?}", events.extrinsic_hash())))
}
#[derive(Parser, Debug)]
#[command(name = "QF faucet bot server")]
#[command(version = "1.0")]
#[command(about = "QF server for crediting by dev tokens", long_about = None)]
struct Cli {
    /// Host can set from env HOST or this option [default: 0.0.0.0]
    #[arg(long = "host", short = 'H')]
    ip: Option<String>,
    /// Port can set from env PORT or this option [default: 8080]
    #[arg(short = 'P', long)]
    port: Option<String>,
    /// RPC url can set from env RPC_URL or this option [default: wss://dev.qfnetwork.xyz/socket]
    #[arg(short, long)]
    rpc_url: Option<String>,
    /// In debug mode the sender is Alice
    #[arg(short, long, action)]
    debug: bool,
    /// Custom delay for transfer in minutes
    #[arg(short, long, default_value = "120")]
    timeout: u64,
}

#[derive(Clone)]
struct ServerConfig {
    rpc_url: String,
    debug: bool,
    timeout: Option<u64>,
}

impl ServerConfig {
    fn new(rpc_url: String, debug: bool, timeout: Option<u64>) -> Self {
        Self {
            rpc_url: rpc_url,
            debug: debug,
            timeout: timeout
        }
    }
}

fn server_config(config: ServerConfig) -> impl Filter<Extract = (ServerConfig,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || config.clone())
}

#[tokio::main]
async fn main() {

    let cli = Cli::parse();

    let _ = env::var("MNEMONIC").expect("MNEMONIC must be set");

    if cli.debug {
        println!("Debug mode is enabled");
        println!("The sender is Alice");
    }

    let rpc_endpoint;
    if cli.rpc_url.is_some() {
        rpc_endpoint = cli.rpc_url.expect("RPC_URL is empty");
    } else {
        rpc_endpoint = env::var("RPC_URL").unwrap_or_else(|_| RPC_URL.to_string());
    }

    let host;
    if cli.ip.is_some() {
        host = cli.ip.expect("HOST is empty");
    } else {
        host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    }
    let port;
    if cli.port.is_some() {
        port = cli.port.unwrap().to_string()
            .parse::<u16>()
            .expect("PORT must be a number");
    } else {
        port = env::var("PORT").unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .expect("PORT must be a number");
    }

    let srv_config = ServerConfig::new(rpc_endpoint.clone(), cli.debug, Some(cli.timeout));

    let hello = warp::path::end()
        .map(|| "QF faucet base server");

    let tokens_route = warp::path!("get" / "tokens")
        .and(warp::post())
        .and(warp::body::json())
        .and(server_config(srv_config.clone()))
        .and_then(transfer_tokens);

    let routes = hello
        .or(tokens_route);

    println!("Server started at {host}:{port} with node address {rpc_endpoint:?}");
    init_db();
    warp::serve(routes)
        .run((host.parse::<std::net::IpAddr>().unwrap(), port))
        .await;
}
