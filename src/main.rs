use anyhow::{Result, bail};
use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use somme_cli::{
    Account, ApiClient, Product, active_account, response_metadata_lines, select_account,
};
use std::path::PathBuf;

const MANPAGE: &str = include_str!("../man/somme.1");

#[derive(Parser)]
#[command(
    name = "somme",
    version,
    about = "Shared authenticated CLI for Somme applications"
)]
struct Cli {
    #[arg(short = 'a', long, default_value = "somme")]
    app: String,
    #[arg(short = 'e', long, default_value = "SOMME")]
    env_prefix: String,
    #[arg(short = 'u', long, default_value = "https://example.invalid/api")]
    api_base: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Login(LoginArgs),
    Logout {
        #[arg(short = 'a', long)]
        account: Option<String>,
    },
    Account {
        #[command(subcommand)]
        command: Option<AccountCommand>,
    },
    Config,
    Man {
        #[command(subcommand)]
        command: Option<ManCommand>,
    },
    Request {
        path: String,
        #[arg(short = 'j', long)]
        json: bool,
    },
}
#[derive(Args)]
struct LoginArgs {
    #[arg(short = 't', long)]
    token: Option<String>,
    #[arg(short = 'u', long)]
    api_base: Option<String>,
    #[arg(short = 'a', long)]
    account: Option<String>,
    #[arg(short = 'e', long)]
    email: Option<String>,
    #[arg(short = 'r', long)]
    tier: Option<String>,
}
#[derive(Subcommand)]
enum AccountCommand {
    Ls,
    Use { name: String },
}

#[derive(Subcommand)]
enum ManCommand {
    Show,
    Install {
        #[arg(short = 'd', long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let product = Product::new(cli.app, cli.env_prefix, cli.api_base);
    match cli.command {
        Command::Login(args) => login(&product, args),
        Command::Logout { account } => logout(&product, account.as_deref()),
        Command::Account { command } => accounts(&product, command),
        Command::Config => show_config(&product),
        Command::Man { command } => man(command),
        Command::Request { path, json } => request(&product, &path, json),
    }
}
fn man(command: Option<ManCommand>) -> Result<()> {
    match command.unwrap_or(ManCommand::Show) {
        ManCommand::Show => {
            print!("{MANPAGE}");
            Ok(())
        }
        ManCommand::Install { dir } => {
            let destination = somme_cli::install_manpage("somme", MANPAGE, dir)?;
            println!("Installed Somme manual at {}", destination.display());
            Ok(())
        }
    }
}
fn login(product: &Product, args: LoginArgs) -> Result<()> {
    let mut config = product.load_config()?;
    let name = args
        .account
        .or_else(|| args.email.clone())
        .or_else(|| config.active_account.clone())
        .unwrap_or_else(|| "default".into());
    if args.token.is_none() {
        let (_, account) = select_account(&config, Some(&name))?;
        if !account.logged_in() {
            bail!("{name} has no saved token; pass --token")
        }
        config.active_account = Some(name.clone());
        product.save_config(&config)?;
        println!("Using {name}");
        return Ok(());
    }
    let api_base = args
        .api_base
        .or_else(|| config.accounts.get(&name).map(|a| a.api_base.clone()))
        .unwrap_or_else(|| product.default_api_base.clone());
    config.accounts.insert(
        name.clone(),
        Account {
            api_base,
            token: args.token,
            email: args.email,
            tier: args.tier,
            updated_at: Some(Utc::now()),
        },
    );
    config.active_account = Some(name.clone());
    product.save_config(&config)?;
    println!("Saved {name}");
    Ok(())
}
fn logout(product: &Product, name: Option<&str>) -> Result<()> {
    let mut c = product.load_config()?;
    let selected = select_account(&c, name)?.0.to_string();
    c.accounts.get_mut(&selected).expect("selected").token = None;
    product.save_config(&c)?;
    println!("Logged out {selected}");
    Ok(())
}
fn accounts(product: &Product, command: Option<AccountCommand>) -> Result<()> {
    let mut c = product.load_config()?;
    match command.unwrap_or(AccountCommand::Ls) {
        AccountCommand::Ls => {
            let selected = active_account(&c).map(|(n, _)| n.to_string());
            for (name, a) in c.accounts {
                println!(
                    "{} {}\t{}",
                    if selected.as_deref() == Some(&name) {
                        "*"
                    } else {
                        " "
                    },
                    name,
                    if a.logged_in() {
                        "logged in"
                    } else {
                        "logged out"
                    }
                )
            }
            Ok(())
        }
        AccountCommand::Use { name } => {
            if !c.accounts.contains_key(&name) {
                bail!("no stored account named {name}")
            }
            c.active_account = Some(name);
            product.save_config(&c)
        }
    }
}
fn show_config(product: &Product) -> Result<()> {
    let c = product.load_config()?;
    println!("app = {}", product.slug);
    println!(
        "account = {}",
        c.active_account.as_deref().unwrap_or("none")
    );
    println!("config_file = {}", product.config_path()?.display());
    println!(
        "tokens = {}",
        c.accounts.values().filter(|a| a.logged_in()).count()
    );
    Ok(())
}
fn request(product: &Product, path: &str, pretty: bool) -> Result<()> {
    let c = product.load_config()?;
    let (_, account) = select_account(&c, None)?;
    let response = ApiClient::from_account(account)?.get(path)?;
    if pretty {
        println!("{}", serde_json::to_string_pretty(&response.body)?)
    } else {
        println!("{}", response.body)
    }
    for line in response_metadata_lines(&response) {
        eprintln!("{line}");
    }
    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use clap::CommandFactory;
    fn assert_short_and_long(command: &clap::Command) {
        for argument in command.get_arguments() {
            if argument.get_long().is_some() {
                assert!(
                    argument.get_short().is_some(),
                    "{} --{} has no short option",
                    command.get_name(),
                    argument.get_long().unwrap()
                );
            }
        }
        for child in command.get_subcommands() {
            assert_short_and_long(child);
        }
    }
    #[test]
    fn every_long_option_has_a_short_option() {
        let command = Cli::command();
        command.clone().debug_assert();
        assert_short_and_long(&command);
    }
    #[test]
    fn embedded_manual_is_section_one() {
        assert!(MANPAGE.contains("SOMME"));
        assert!(MANPAGE.contains("\\-\\-token"));
    }
}
