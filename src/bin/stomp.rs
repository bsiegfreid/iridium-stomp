use clap::Parser;
use iridium_stomp::{Connection, Frame};
use std::io::{self, BufRead, Write};
use tokio::sync::mpsc;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    println!("Connecting to {}...", cli.address);
    let conn = Connection::connect(&cli.address, &cli.login, &cli.passcode, &cli.heartbeat).await?;
    println!("Connected.");

    // Subscribe to requested destinations
    for dest in &cli.subscribe {
        let sub = conn
            .subscribe(dest, iridium_stomp::connection::AckMode::Auto)
            .await?;
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
        io::stdout().flush()?;

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

    Ok(())
}
