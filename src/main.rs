use anyhow::Result;
use jtools::app::state::DesktopPlatform;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root_dir = PathBuf::from("./runtime");
    let mut platform = DesktopPlatform::bootstrap(&root_dir).await?;
    let report = platform.startup_report();

    if args.is_empty() {
        println!(
            "jtools runtime initialized at {}",
            report.root_dir.display()
        );
        println!(
            "plugins loaded: total={}, enabled={}, disabled={}, faulted={}",
            report.loaded_plugins.total,
            report.loaded_plugins.enabled,
            report.loaded_plugins.disabled,
            report.loaded_plugins.faulted
        );
        println!("global hotkey: {}", report.hotkey);
        println!("usage:");
        println!("  cargo run -- search <keyword>");
        println!("  cargo run -- exec <keyword>");
        return Ok(());
    }

    match args[0].as_str() {
        "search" => {
            let query = args[1..].join(" ");
            let results = platform.search(query).await;
            for (index, item) in results.iter().enumerate() {
                println!(
                    "{}. [{}] {} - {}",
                    index + 1,
                    item.source_type,
                    item.title,
                    item.subtitle
                );
            }
        }
        "exec" => {
            let query = args[1..].join(" ");
            let results = platform.search(query).await;
            if let Some(item) = results.first() {
                let message = platform.execute(item).await?;
                println!("{message}");
            } else {
                println!("no result found");
            }
        }
        command => {
            println!("unknown command: {command}");
        }
    }

    Ok(())
}
