use clap::Parser;

use serde_json::Value;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    analyze: bool,
}

async fn request_model(
    client: &reqwest::Client,
    api_key: &str,
    diff: &str,
) -> anyhow::Result<serde_json::Value> {
    let res = client
        .post("https://ai.hackclub.com/proxy/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": "google/gemini-2-flash-preview",
            "messages": [
                {
                    "role": "system",
                    "content": r#"You are DevDiff, an expert code review assistant embedded in a developer's terminal.

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
                    "#
                },
                {
                    "role": "user",
                    "content": "Hello!"
                }
            ]
        }))
        .send()
        .await?;

    let json = res.json::<serde_json::Value>().await?;
    Ok(json)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Load environment variables from .env file
    dotenvy::dotenv().ok();
    let api_key = std::env::var("MODEL_API_KEY").expect("API_KEY not set");

    // Create a reqwest client
    let client = reqwest::Client::new();

    let res = request_model(&client, &api_key, "Hii").await?;
    let content = res["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");

    println!("\n\n\n{}", content);

    Ok(())
}
