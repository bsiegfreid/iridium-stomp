use clap::Parser;
use iridium_stomp::connection::ConnError;
use iridium_stomp::{Connection, Frame};
use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use tokio::sync::mpsc;

/// Exit codes for different error conditions
mod exit_codes {
    /// Successful execution
    pub const SUCCESS: u8 = 0;
    /// Network/connection error (e.g., host unreachable, connection refused)
    pub const NETWORK_ERROR: u8 = 1;
    /// Authentication error (e.g., invalid credentials)
    pub const AUTH_ERROR: u8 = 2;
    /// Protocol error (e.g., unexpected server response)
    pub const PROTOCOL_ERROR: u8 = 3;
}

#[derive(Parser)]
#[command(name = "stomp")]
#[command(about = "Interactive STOMP client CLI")]
struct Cli {
    /// STOMP broker address (host:port)
    #[arg(short, long, default_value = "127.0.0.1:61613")]
    address: String,

    /// Login username
    #[arg(short, long, default_value = "guest")]
    login: String,

    /// Passcode
    #[arg(short, long, default_value = "guest")]
    passcode: String,

    /// Heartbeat settings (client-send,client-receive in ms)
    #[arg(long, default_value = "10000,10000")]
    heartbeat: String,

    /// Destinations to subscribe to (can be specified multiple times)
    #[arg(short, long)]
    subscribe: Vec<String>,
}

/// Format a connection error with user-friendly messaging
fn format_connection_error(err: &ConnError, address: &str) -> (String, u8) {
    match err {
        ConnError::Io(io_err) => {
            let message = match io_err.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    format!("Connection refused: {}", address)
                }
                std::io::ErrorKind::TimedOut => {
                    format!("Connection timed out: {}", address)
                }
                _ => {
                    format!("Connection failed: {}", io_err)
                }
            };
            (message, exit_codes::NETWORK_ERROR)
        }
        ConnError::ServerRejected(server_err) => {
            let mut message = format!("Authentication failed: {}", server_err.message);
            if let Some(body) = &server_err.body {
                message.push_str(&format!(" ({})", body));
            }
            (message, exit_codes::AUTH_ERROR)
        }
        ConnError::Protocol(msg) => (
            format!("Protocol error: {}", msg),
            exit_codes::PROTOCOL_ERROR,
        ),
        ConnError::ReceiptTimeout(id) => (
            format!("Receipt timeout: {}", id),
            exit_codes::PROTOCOL_ERROR,
        ),
    }
}

/// Print an error message to stderr
fn print_error(message: &str) {
    eprintln!("{}", message);
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    println!("Connecting to {}...", cli.address);

    let conn =
        match Connection::connect(&cli.address, &cli.login, &cli.passcode, &cli.heartbeat).await {
            Ok(conn) => conn,
            Err(err) => {
                let (message, exit_code) = format_connection_error(&err, &cli.address);
                print_error(&message);
                return ExitCode::from(exit_code);
            }
        };

    println!("Connected.");

    // Subscribe to requested destinations
    for dest in &cli.subscribe {
        let sub = match conn
            .subscribe(dest, iridium_stomp::connection::AckMode::Auto)
            .await
        {
            Ok(sub) => sub,
            Err(err) => {
                print_error(&format!("Failed to subscribe to '{}': {}", dest, err));
                return ExitCode::from(exit_codes::PROTOCOL_ERROR);
            }
        };
        println!("Subscribed to: {}", dest);

        // Spawn a task to print incoming messages for this subscription
        let dest_clone = dest.clone();
        let mut rx = sub.into_receiver();
        tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                println!("\n[{}] MESSAGE received:", dest_clone);
                for (k, v) in &frame.headers {
                    println!("  {}: {}", k, v);
                }
                if !frame.body.is_empty() {
                    match std::str::from_utf8(&frame.body) {
                        Ok(s) => println!("  Body: {}", s),
                        Err(_) => println!("  Body: ({} bytes, binary)", frame.body.len()),
                    }
                }
                print!("> ");
                let _ = io::stdout().flush();
            }
        });
    }

    // Channel to receive user commands from stdin reader
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<String>(16);

    // Spawn blocking stdin reader
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let reader = stdin.lock();
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if cmd_tx.blocking_send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    println!();
    println!("Commands:");
    println!("  send <destination> <message>  - Send a message");
    println!("  sub <destination>             - Subscribe to a destination");
    println!("  quit                          - Exit");
    println!();

    loop {
        print!("> ");
        let _ = io::stdout().flush();

        let line = match cmd_rx.recv().await {
            Some(l) => l,
            None => break,
        };

        let parts: Vec<&str> = line.trim().splitn(3, ' ').collect();
        if parts.is_empty() || parts[0].is_empty() {
            continue;
        }

        match parts[0] {
            "quit" | "exit" | "q" => {
                println!("Disconnecting...");
                conn.close().await;
                break;
            }
            "send" => {
                if parts.len() < 3 {
                    println!("Usage: send <destination> <message>");
                    continue;
                }
                let dest = parts[1];
                let msg = parts[2];
                let frame = Frame::new("SEND")
                    .header("destination", dest)
                    .header("content-type", "text/plain")
                    .set_body(msg.as_bytes().to_vec());
                match conn.send_frame(frame).await {
                    Ok(_) => println!("Sent to {}", dest),
                    Err(e) => println!("Send error: {}", e),
                }
            }
            "sub" | "subscribe" => {
                if parts.len() < 2 {
                    println!("Usage: sub <destination>");
                    continue;
                }
                let dest = parts[1].to_string();
                match conn
                    .subscribe(&dest, iridium_stomp::connection::AckMode::Auto)
                    .await
                {
                    Ok(sub) => {
                        println!("Subscribed to: {}", dest);
                        let dest_clone = dest.clone();
                        let mut rx = sub.into_receiver();
                        tokio::spawn(async move {
                            while let Some(frame) = rx.recv().await {
                                println!("\n[{}] MESSAGE received:", dest_clone);
                                for (k, v) in &frame.headers {
                                    println!("  {}: {}", k, v);
                                }
                                if !frame.body.is_empty() {
                                    match std::str::from_utf8(&frame.body) {
                                        Ok(s) => println!("  Body: {}", s),
                                        Err(_) => {
                                            println!("  Body: ({} bytes, binary)", frame.body.len())
                                        }
                                    }
                                }
                                print!("> ");
                                let _ = io::stdout().flush();
                            }
                        });
                    }
                    Err(e) => println!("Subscribe error: {}", e),
                }
            }
            "help" | "?" => {
                println!("Commands:");
                println!("  send <destination> <message>  - Send a message");
                println!("  sub <destination>             - Subscribe to a destination");
                println!("  quit                          - Exit");
            }
            _ => {
                println!("Unknown command: {}. Type 'help' for commands.", parts[0]);
            }
        }
    }

    ExitCode::from(exit_codes::SUCCESS)
}
