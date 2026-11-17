//! telerust - Telegram user client in Rust

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use telerust_core::{config::AppConfig, TelegramClient, HighLevelClient, TelegramClientConfig, Chat, ChatType, Message, User, LoginToken};

#[derive(Parser)]
#[command(name = "telerust")]
#[command(about = "Telegram user client in Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Set log level
    #[arg(short = 'L', long, global = true, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Login to Telegram (first time setup)
    Login {
        /// Phone number in international format (e.g., +1234567890)
        #[arg(short, long)]
        phone: String,
    },
    /// List dialogs/chats
    Dialogs {
        /// Number of dialogs to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Send a message
    Send {
        /// Chat ID or username
        #[arg(short, long)]
        chat: String,

        /// Message text
        #[arg(short, long)]
        text: String,
    },
    /// Get messages from a chat
    Messages {
        /// Chat ID or username
        #[arg(short, long)]
        chat: String,

        /// Number of messages to fetch
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },
    /// Show current user info
    Me,
    /// Run the client (listen for updates)
    Run,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load config
    let config = AppConfig::load()?;

    if config.telegram.api_id == 0 || config.telegram.api_hash.is_empty() {
        eprintln!("Error: Telegram API credentials not configured!");
        eprintln!("Please set TELERUST__TELEGRAM__API_ID and TELERUST__TELEGRAM__API_HASH environment variables");
        eprintln!("Or create a config file at ~/.config/telerust/config.toml");
        eprintln!();
        eprintln!("Get your API credentials from https://my.telegram.org");
        std::process::exit(1);
    }

    // Create client
    let client_config = TelegramClientConfig {
        api_id: config.telegram.api_id,
        api_hash: config.telegram.api_hash.clone(),
        session_file: config.session_file(),
        download_dir: config.app.download_dir.clone(),
    };

    let client = TelegramClient::new(client_config).await?;
    let high_level = HighLevelClient::new(client).await;

    match cli.command {
        Commands::Login { phone } => {
            login(&high_level, &phone).await?;
        }
        Commands::Dialogs { limit } => {
            list_dialogs(&high_level, limit).await?;
        }
        Commands::Send { chat, text } => {
            send_message(&high_level, &chat, &text).await?;
        }
        Commands::Messages { chat, limit } => {
            get_messages(&high_level, &chat, limit).await?;
        }
        Commands::Me => {
            show_me(&high_level).await?;
        }
        Commands::Run => {
            run_client(&high_level).await?;
        }
    }

    Ok(())
}

async fn login(high_level: &HighLevelClient, phone: &str) -> Result<()> {
    info!("Logging in with phone: {}", phone);

    let token: LoginToken = high_level.request_login_code(phone).await?;
    println!("Login code sent to {}", phone);

    // Get code from user
    print!("Enter the code you received: ");
    use std::io::{self, Write};
    io::stdout().flush()?;
    let mut code = String::new();
    io::stdin().read_line(&mut code)?;
    let code = code.trim();

    // Sign in
    high_level.sign_in(token, code).await?;
    println!("Successfully logged in!");

    // Save session
    high_level.sync_update_state().await?;

    Ok(())
}

async fn list_dialogs(high_level: &HighLevelClient, limit: usize) -> Result<()> {
    let dialogs: Vec<Chat> = high_level.get_dialogs().await?;

    println!("Your dialogs ({} total):", dialogs.len());
    for (i, dialog) in dialogs.iter().take(limit).enumerate() {
        let chat_type = match dialog.chat_type {
            ChatType::Channel => "Channel",
            ChatType::Supergroup => "Supergroup",
            ChatType::Group => "Group",
            ChatType::Community => "Community",
            ChatType::Private => "Private",
        };
        let username = dialog.username.as_deref().unwrap_or("none");
        println!("  {}. {} [{}] (@{}) - ID: {}",
            i + 1, dialog.title, chat_type, username, dialog.id);
    }

    Ok(())
}

async fn send_message(high_level: &HighLevelClient, chat: &str, text: &str) -> Result<()> {
    // Try to parse as chat ID first, then try username
    let chat_id = if let Ok(id) = chat.parse::<i64>() {
        id
    } else {
        // Resolve username
        if let Some(user) = high_level.resolve_username(chat).await? {
            user.id
        } else {
            anyhow::bail!("Could not resolve chat: {}", chat);
        }
    };

    let msg: Message = high_level.send_message(chat_id, text).await?;
    println!("Message sent! ID: {}", msg.id);

    Ok(())
}

async fn get_messages(high_level: &HighLevelClient, chat: &str, limit: usize) -> Result<()> {
    let chat_id = if let Ok(id) = chat.parse::<i64>() {
        id
    } else {
        if let Some(user) = high_level.resolve_username(chat).await? {
            user.id
        } else {
            anyhow::bail!("Could not resolve chat: {}", chat);
        }
    };

    let messages: Vec<Message> = high_level.get_messages(chat_id, limit).await?;

    println!("Messages in chat {} ({} total):", chat_id, messages.len());
    for msg in messages.iter().rev() {
        let direction = if msg.is_outgoing { "→" } else { "←" };
        println!("  {} [{}] {}: {}", direction, msg.id, msg.sender_id.unwrap_or(0), msg.text);
    }

    Ok(())
}

async fn show_me(high_level: &HighLevelClient) -> Result<()> {
    let me: User = high_level.get_me().await?;

    println!("Your account:");
    println!("  ID: {}", me.id);
    println!("  Name: {} {}", me.first_name, me.last_name.unwrap_or_default());
    if let Some(username) = me.username {
        println!("  Username: @{}", username);
    }
    println!("  Bot: {}", me.is_bot);

    Ok(())
}

async fn run_client(high_level: &HighLevelClient) -> Result<()> {
    info!("Starting client... Press Ctrl+C to stop");

    high_level.run_updates(|update| {
        telerust_core::handlers::handle_update(update);
    }).await?;

    Ok(())
}