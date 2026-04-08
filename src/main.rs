use clap::{Parser, Subcommand};
use std::io::Write;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    staged: bool,
    #[arg(short, long, default_value_t = 1)]
    number: u32,
    #[arg(short, long)]
    raw: bool,
    #[arg(long)]
    hash: Option<String>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Init,
}

const SHARED_RULES: &str = r#"Rules you must follow:
- Start response with "SUMMARY"
- Be direct, technical
- Never explain git
- No filler phrases
- If diff empty, say "Nothing to analyze."
- If only formatting, say so
- Output must be scannable
- Lines starting `-` are deleted; do not report issues with them"#;

fn build_prompt(extra: &str) -> String {
    format!(
        r#"You are DevDiff, an expert code review assistant.

Analyze a git diff and produce a concise structured summary.

SUMMARY
CHANGES
ARCHITECTURAL IMPACT
POTENTIAL ISSUES

{}

{}"#,
        extra, SHARED_RULES
    )
}

async fn request_model(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    content: &str,
    type_: u8,
) -> anyhow::Result<()> {
    let extra = if type_ == 1 {
        "SUGGESTED COMMIT MESSAGE: single conventional commit message."
    } else {
        ""
    };

    let system_prompt = build_prompt(extra);

    let res = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": model,
            "max_tokens": 2048,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": format!("Analyze this git diff:\n\n{}", content)}
            ]
        }))
        .send()
        .await?;

    let json: serde_json::Value = res.json().await?;
    let ai_response = json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|s| s.as_str())
        .unwrap_or("Error: could not parse AI response");

    println!("{}", ai_response);
    Ok(())
}

fn get_diff_for_commits(number: u32) -> anyhow::Result<String> {
    let repo = git2::Repository::open(".")?;
    let mut commit = repo.head()?.peel_to_commit()?;
    let mut diff_text = String::new();

    for i in 0..number {
        let commit_tree = commit.tree()?;
        let parent = commit.parent(0)?;
        let parent_tree = parent.tree()?;
        let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)?;
        if number > 1 {
            diff_text.push_str(&format!(
                "\n--- Commit {} ({}) ---\n",
                i + 1,
                &commit.id().to_string()[..7]
            ));
        }
        diff.print(git2::DiffFormat::Patch, |_d, _h, line| {
            diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
            true
        })?;
        commit = parent;
    }
    Ok(diff_text)
}

fn get_diff_for_hash(hash: &str) -> anyhow::Result<String> {
    let repo = git2::Repository::open(".")?;
    let commit = repo.find_commit(git2::Oid::from_str(hash)?)?;
    let parent_tree = commit.parent(0)?.tree()?;
    let commit_tree = commit.tree()?;
    let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)?;
    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_d, _h, line| {
        diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
        true
    })?;
    Ok(diff_text)
}

fn get_staged() -> anyhow::Result<String> {
    let repo = git2::Repository::open(".")?;
    let head_tree = repo.head()?.peel_to_commit()?.tree()?;
    let index = repo.index()?;
    let diff = repo.diff_tree_to_index(Some(&head_tree), Some(&index), None)?;
    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_d, _h, line| {
        diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
        true
    })?;
    Ok(diff_text)
}

fn run_init() -> anyhow::Result<()> {
    let config_dir = dirs::home_dir()
        .expect("No home dir")
        .join(".config/devdiff");
    std::fs::create_dir_all(&config_dir)?;
    let env_path = config_dir.join(".env");

    print!("Enter OpenRouter model name: ");
    std::io::stdout().flush()?;
    let mut model = String::new();
    std::io::stdin().read_line(&mut model)?;
    let model = model.trim();
    if model.is_empty() {
        eprintln!("Model cannot be empty");
        std::process::exit(1);
    }

    print!("Enter OpenRouter API key: ");
    std::io::stdout().flush()?;
    let mut api_key = String::new();
    std::io::stdin().read_line(&mut api_key)?;
    let api_key = api_key.trim();
    if api_key.is_empty() {
        eprintln!("API key cannot be empty");
        std::process::exit(1);
    }

    std::fs::write(
        &env_path,
        format!("MODEL_API_KEY={}\nMODEL_NAME={}\n", api_key, model),
    )?;
    println!("✓ Config saved to {}", env_path.display());
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if let Some(Commands::Init) = args.command {
        return run_init();
    }

    let config_path = dirs::home_dir()
        .expect("No home dir")
        .join(".config/devdiff/.env");
    dotenvy::from_path(config_path).ok();

    let api_key = std::env::var("MODEL_API_KEY").expect("API_KEY not set — run `devdiff init`");
    let model = std::env::var("MODEL_NAME").expect("MODEL_NAME not set — run `devdiff init`");
    let client = reqwest::Client::new();

    if args.staged && args.number > 1
        || args.staged && args.hash.is_some()
        || args.hash.is_some() && args.number > 1
    {
        eprintln!("Invalid argument combination");
        std::process::exit(1);
    }

    let diff = if args.staged {
        get_staged()?
    } else if let Some(ref hash) = args.hash {
        get_diff_for_hash(hash)?
    } else {
        get_diff_for_commits(args.number)?
    };

    if args.raw {
        println!("{}", diff);
        return Ok(());
    }

    let type_ = if args.staged { 1 } else { 0 };
    request_model(&client, &api_key, &model, &diff, type_).await?;
    Ok(())
}
