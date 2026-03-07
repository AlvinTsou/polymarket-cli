use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const MAX_CONTENT_CHARS: usize = 30_000;
const GEMINI_MODEL: &str = "gemini-2.0-flash";

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<Content>,
    system_instruction: Content,
}

#[derive(Serialize, Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
    error: Option<GeminiError>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Content,
}

#[derive(Deserialize)]
struct GeminiError {
    message: String,
}

const SYSTEM_PROMPT: &str = r#"你是一個專業的文章摘要助手。請用繁體中文為使用者整理文章重點。

請按照以下格式輸出：

## 標題
（文章標題或主題）

## 重點摘要
- 列出 3-5 個關鍵要點
- 每個要點用一句話概括

## 核心觀點
用 2-3 句話總結文章的核心論點或發現

## 實用建議
- 如果文章有可操作的建議，列出 1-3 個
- 如果沒有，可以省略此區塊

請保持簡潔，讓讀者能在 30 秒內掌握文章精華。"#;

pub async fn summarize(api_key: &str, article_text: &str) -> Result<String> {
    let truncated: String = article_text.chars().take(MAX_CONTENT_CHARS).collect();

    let request = GeminiRequest {
        contents: vec![Content {
            parts: vec![Part { text: truncated }],
        }],
        system_instruction: Content {
            parts: vec![Part {
                text: SYSTEM_PROMPT.to_string(),
            }],
        },
    };

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{GEMINI_MODEL}:generateContent?key={api_key}"
    );

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .context("Failed to call Gemini API")?;

    let status = response.status();
    let body: GeminiResponse = response
        .json()
        .await
        .context("Failed to parse Gemini API response")?;

    if let Some(err) = body.error {
        bail!("Gemini API error ({}): {}", status, err.message);
    }

    let candidates = body
        .candidates
        .context("No candidates in Gemini response")?;
    let candidate = candidates.first().context("Empty candidates list")?;
    let text = candidate
        .content
        .parts
        .first()
        .context("No parts in response")?;

    Ok(text.text.clone())
}
