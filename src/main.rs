use clap::Parser;
use futures_util::StreamExt;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    staged: bool,
}

async fn request_model(
    client: &reqwest::Client,
    api_key: &str,
    content: &str,
    type_: u8,
) -> anyhow::Result<()> {
    let type0 = r#"You are DevDiff, an expert code review assistant embedded in a developer's terminal.

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

        Rules you must follow:
        - Be direct and technical. The user is a developer, not a manager.
        - Never explain what a diff is or how Git works.
        - Never add encouragement, praise, or filler phrases like "Great change!" or "This looks good!".
        - If the diff is empty or contains no meaningful changes, say "Nothing to analyze." and stop.
        - If the diff contains only whitespace or formatting changes, say so explicitly.
        - Keep your total output scannable. This is a terminal tool — walls of text are useless.
        - Lines beginning with `-` in the diff are REMOVED code. Do not flag issues in removed lines — they no longer exist in the codebase. Only analyze lines beginning with `+` (added) and ` ` (context)."#;
    let type1 = r#"You are DevDiff, an expert code review assistant embedded in a developer's terminal.

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

        SUGGESTED COMMIT MESSAGE
        Write a single conventional commit message that accurately describes this change. Format: type(scope): description. Example: feat(auth): add token refresh on 401 response.

        Rules you must follow:
        - Be direct and technical. The user is a developer, not a manager.
        - Never explain what a diff is or how Git works.
        - Never add encouragement, praise, or filler phrases like "Great change!" or "This looks good!".
        - If the diff is empty or contains no meaningful changes, say "Nothing to analyze." and stop.
        - If the diff contains only whitespace or formatting changes, say so explicitly.
        - Keep your total output scannable. This is a terminal tool — walls of text are useless.
        - The diff uses standard Git format. Lines starting with `-` are DELETED and DO NOT EXIST in the current codebase. Lines starting with `+` are ADDED and represent the current state. Lines starting with ` ` (space) are unchanged context. You must NEVER report issues about `-` lines. If you do, you are wrong."#;

    // type 0 -> latest commit
    // type 1 -> staged changes
    // type 2 -> unstaged changes
    // type 3 -> selected commit

    let system_prompt = match type_ {
        0 => type0,
        1 => type1,
        _ => type0,
    };

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
        let text = std::str::from_utf8(&chunk).unwrap_or("");

        for line in text.lines() {
            if line.starts_with("data: ") {
                let data = &line["data: ".len()..];
                if data == "[DONE]" {
                    break;
                }
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(token) = json["choices"][0]["delta"]["content"].as_str() {
                        print!("{}", token);
                        use std::io::Write;
                        std::io::stdout().flush()?;
                    }
                }
            }
        }
    }

    println!();
    Ok(())
}

fn get_last_diff() -> anyhow::Result<String> {
    let repo = git2::Repository::open(".")?;
    let head = repo.head()?.peel_to_commit()?;
    let head_tree = head.tree()?;

    let parent = head.parent(0)?;
    let parent_tree = parent.tree()?;

    let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&head_tree), None)?;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Load environment variables from .env file
    let config_path = dirs::home_dir().unwrap().join(".config/devdiff/.env");
    dotenvy::from_path(config_path).ok();

    let api_key = std::env::var("MODEL_API_KEY").expect("API_KEY not set");

    // Create a reqwest client
    let client = reqwest::Client::new();

    if args.staged {
        let staged = get_staged()?;
        request_model(&client, &api_key, &staged, 1).await?;
        println!("\n\n\nAI and can make mistakes. Please double-check responses.")
    } else {
        let diff = get_last_diff()?;
        request_model(&client, &api_key, &diff, 0).await?;
        println!("\n\n\nAI and can make mistakes. Please double-check responses.")
    }

    Ok(())
}
