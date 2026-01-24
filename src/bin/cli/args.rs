use clap::Parser;

#[derive(Parser)]
#[command(name = "stomp")]
#[command(version)]
#[command(about = "Interactive STOMP client CLI")]
pub struct Cli {
    /// STOMP broker address (host:port)
    #[arg(short, long, default_value = "127.0.0.1:61613")]
    pub address: String,

    /// Login username
    #[arg(short, long, default_value = "guest")]
    pub login: String,

    /// Passcode
    #[arg(short, long, default_value = "guest")]
    pub passcode: String,

    /// Heartbeat settings (client-send,client-receive in ms)
    #[arg(long, default_value = "10000,10000")]
    pub heartbeat: String,

    /// Destinations to subscribe to (can be specified multiple times)
    #[arg(short, long)]
    pub subscribe: Vec<String>,

    /// Enable TUI mode with panels and live updates
    #[arg(long)]
    pub tui: bool,

    /// Show session summary on exit
    #[arg(long)]
    pub summary: bool,
}
