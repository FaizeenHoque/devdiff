use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use std::io::Write;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    staged: bool,

    #[arg(short, long, default_value_t = 1)]
    number: u32,

    #[arg(short, long)]
    raw: bool,

    /// Analyze a specific commit by hash
    #[arg(long)]
    hash: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Set up DevDiff config interactively
    Init,
}

const SHARED_RULES: &str = r#"Rules you must follow:
        - Be direct and technical. The user is a developer, not a manager.
        - Never explain what a diff is or how Git works.
        - Never add encouragement, praise, or filler phrases like "Great change!" or "This looks good!".
        - If the diff is empty or contains no meaningful changes, say "Nothing to analyze." and stop.
        - If the diff contains only whitespace or formatting changes, say so explicitly.
        - Keep your total output scannable. This is a terminal tool — walls of text are useless.
        - The diff uses standard Git format. Lines starting with `-` are DELETED and DO NOT EXIST in the current codebase. Lines starting with `+` are ADDED and represent the current state. Lines starting with ` ` (space) are unchanged context. You must NEVER report issues about `-` lines. If you do, you are wrong."#;

fn build_prompt(extra: &str) -> String {
    format!(
        r#"You are DevDiff, an expert code review assistant embedded in a developer's terminal.

        You will be given a Git diff as input. Your job is to analyze it and produce a concise, structured summary that helps the developer understand what changed, why it matters, and what to watch out for.

        Your output must always follow this structure:

        SUMMARY
        One to three sentences. What changed at a high level, in plain English. No jargon unless necessary.

        CHANGES
        A bullet list of the specific changes made. Be concrete — mention function names, file names, and logic changes. Do not just restate the diff line by line. Group related changes together.

        ARCHITECTURAL IMPACT
        How does this change affect the overall structure of the codebase? Does it introduce new dependencies? Does it change control flow? Does it affect performance, security, or maintainability? If the diff is too small to have architectural impact, say "None significant."

        POTENTIAL ISSUES
        Be ruthless here. List anything that looks wrong, risky, or incomplete. Unhandled errors, missing edge cases, hardcoded values, race conditions, anything suspicious. If nothing looks wrong, say "None detected." Do not fabricate issues.

        {}

        {}"#,
        extra, SHARED_RULES
    )
}

async fn request_model(
    client: &reqwest::Client,
    api_key: &str,
    content: &str,
    type_: u8,
) -> anyhow::Result<()> {
    let extra = match type_ {
        1 => {
            "SUGGESTED COMMIT MESSAGE\n        Write a single conventional commit message that accurately describes this change. Format: type(scope): description. Example: feat(auth): add token refresh on 401 response."
        }
        _ => "",
    };

    let system_prompt = build_prompt(extra);

    let res = client
        .post("https://ai.hackclub.com/proxy/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": "google/gemini-3-flash-preview",
            "stream": true,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": format!("Analyze this git diff:\n\n{}", content)}
            ]
        }))
        .send()
        .await?;

    let mut stream = res.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let text = String::from_utf8_lossy(&chunk);

        for line in text.lines() {
            if line.starts_with("data: ") {
                let data = &line["data: ".len()..];
                if data == "[DONE]" {
                    break;
                }
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(token) = json["choices"][0]["delta"]["content"].as_str() {
                        print!("{}", token);
                        std::io::stdout().flush()?;
                    }
                }
            }
        }
    }

    println!();
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

        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let content = std::str::from_utf8(line.content()).unwrap_or("");
            diff_text.push_str(content);
            true
        })?;

        commit = parent;
    }

    Ok(diff_text)
}

fn get_diff_for_hash(hash: &str) -> anyhow::Result<String> {
    let repo = git2::Repository::open(".")?;
    let oid = git2::Oid::from_str(hash)?;
    let commit = repo.find_commit(oid)?;
    let commit_tree = commit.tree()?;

    let parent = commit.parent(0)?;
    let parent_tree = parent.tree()?;

    let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)?;

    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let content = std::str::from_utf8(line.content()).unwrap_or("");
        diff_text.push_str(content);
        true
    })?;

    Ok(diff_text)
}

fn get_staged() -> anyhow::Result<String> {
    let repo = git2::Repository::open(".")?;
    let head = repo.head()?.peel_to_commit()?;
    let head_tree = head.tree()?;

    let index = repo.index()?;
    let diff = repo.diff_tree_to_index(Some(&head_tree), Some(&index), None)?;

    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let content = std::str::from_utf8(line.content()).unwrap_or("");
        diff_text.push_str(content);
        true
    })?;

    Ok(diff_text)
}

fn run_init() -> anyhow::Result<()> {
    let config_dir = dirs::home_dir()
        .expect("Could not find home directory")
        .join(".config/devdiff");

    std::fs::create_dir_all(&config_dir)?;

    let env_path = config_dir.join(".env");

    print!("Enter your API key: ");
    std::io::stdout().flush()?;

    let mut api_key = String::new();
    std::io::stdin().read_line(&mut api_key)?;
    let api_key = api_key.trim();

    if api_key.is_empty() {
        eprintln!("Error: API key cannot be empty");
        std::process::exit(1);
    }

    std::fs::write(&env_path, format!("MODEL_API_KEY={}\n", api_key))?;

    println!("✓ Config saved to {}", env_path.display());
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Handle init subcommand before anything else
    if let Some(Commands::Init) = args.command {
        return run_init();
    }

    let config_path = dirs::home_dir()
        .expect("Could not find home directory")
        .join(".config/devdiff/.env");
    dotenvy::from_path(config_path).ok();

    let api_key = std::env::var("MODEL_API_KEY").expect("API_KEY not set — run `devdiff init`");
    let client = reqwest::Client::new();

    // Validate flag combos
    if args.staged && args.number > 1 {
        eprintln!("Error: --staged and --number cannot be used together");
        std::process::exit(1);
    }
    if args.staged && args.hash.is_some() {
        eprintln!("Error: --staged and --hash cannot be used together");
        std::process::exit(1);
    }

    // Get the diff
    let diff = if args.staged {
        get_staged()?
    } else if let Some(ref hash) = args.hash {
        get_diff_for_hash(hash)?
    } else {
        get_diff_for_commits(args.number)?
    };

    // Raw mode — just print the diff
    if args.raw {
        println!("{}", diff);
        return Ok(());
    }

    // AI analysis
    let type_ = if args.staged { 1 } else { 0 };
    request_model(&client, &api_key, &diff, type_).await?;

    println!("\n⚠  AI can make mistakes. Double-check responses.");
    Ok(())
}
