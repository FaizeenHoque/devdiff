# DevDiff

## Introduction

DevDiff is an AI powered CLI tool that reviews your git diffs and points out mistakes.

It reads the changes in your repository and produces a structured summary. You see what changed, why it matters, and what looks risky. It does not explain git. It focuses on finding problems.

You run it in your terminal before committing or after. It helps you catch issues early and understand the impact of changes across your codebase.

Give it a diff. It returns analysis.

---

## Install

You need Rust installed.

If you do not have Rust:

```bash
curl https://sh.rustup.rs -sSf | sh
```

Clone the repository:

```bash
git clone https://github.com/FaizeenHoque/devdiff
cd devdiff
```

Build the binary:

```bash
cargo build --release
```

Move the binary somewhere in your PATH:

```bash
mv target/release/devdiff /usr/local/bin/
```

Check installation:

```bash
devdiff --help
```

---

## Setup

Initialize configuration:

```bash
devdiff init
```

You will be prompted for:

1. **OpenRouter model name** – e.g., `nvidia/nemotron-3-super-120b-a12b:free`.
2. **OpenRouter API key** – your personal API key from OpenRouter.

The credentials are saved in:

```
~/.config/devdiff/.env
```

Example `.env` file contents:

```
MODEL_NAME=nvidia/nemotron-3-super-120b-a12b:free
MODEL_API_KEY=your_api_key_here
```

---

## Usage

Analyze last commit:

```bash
devdiff
```

Analyze multiple commits:

```bash
devdiff --number 3
```

Analyze staged changes:

```bash
devdiff --staged
```

Analyze a specific commit:

```bash
devdiff --hash <commit_hash>
```

Print raw diff:

```bash
devdiff --raw
```

DevDiff prints structured output directly in your terminal.

---

## How it works

DevDiff reads git diffs using libgit2.

It sends the diff to the AI model you configured.

The model returns:

**SUMMARY**
High level description of the change.

**CHANGES**
Specific modifications grouped logically.

**ARCHITECTURAL IMPACT**
Effect on structure, performance, or maintainability.

**POTENTIAL ISSUES**
Anything risky or incorrect.

When you analyze staged changes, DevDiff also suggests a commit message.

---

## Build from source

Clone the repo:

```bash
git clone https://github.com/FaizeenHoque/devdiff
cd devdiff
```

Build:

```bash
cargo build
```

Run locally:

```bash
cargo run -- --help
```

---

## Credits

Built by a developer who got tired of reviewing their own diffs.

Uses:

* Rust
* clap
* reqwest
* git2
* dotenvy

If something breaks, the bug was already in your code. DevDiff just found it.
