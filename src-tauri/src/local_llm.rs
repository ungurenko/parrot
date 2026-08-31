use std::io::{BufRead, BufReader};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::cancellation::CancelToken;
use crate::mlx_env;

static SERVER_CLIENT: Lazy<Result<reqwest::blocking::Client, reqwest::Error>> = Lazy::new(|| {
    reqwest::blocking::Client::builder()
        .timeout(mlx_env::SERVER_REQUEST_TIMEOUT)
        .build()
});

#[derive(Clone, Copy)]
pub struct GenerationOptions {
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
}

pub fn generate(
    url: &str,
    server_pid: u32,
    system_prompt: &str,
    user_prompt: &str,
    options: GenerationOptions,
    cancel: Arc<CancelToken>,
) -> Result<String> {
    if cancel.is_cancelled() {
        anyhow::bail!("cancelled");
    }

    cancel.register_pid(server_pid);
    let result = generate_inner(url, system_prompt, user_prompt, options, cancel.clone());
    cancel.unregister_pid(server_pid);
    result
}

pub fn warmup(url: &str, options: GenerationOptions) -> Result<()> {
    let response = server_client()?
        .post(format!("{url}/v1/chat/completions"))
        .json(&ServerRequest {
            messages: vec![ChatMessage {
                role: "user",
                content: "Привет",
            }],
            max_tokens: 4,
            temperature: options.temperature,
            top_p: options.top_p,
            stream: false,
        })
        .send()
        .context("Не удалось прогреть сервер локальной модели")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(anyhow!(
            "Сервер локальной модели на прогреве вернул {status}: {body}"
        ));
    }
    let _ = response.bytes()?;
    Ok(())
}

fn generate_inner(
    url: &str,
    system_prompt: &str,
    user_prompt: &str,
    options: GenerationOptions,
    cancel: Arc<CancelToken>,
) -> Result<String> {
    let response = server_client()?
        .post(format!("{url}/v1/chat/completions"))
        .json(&ServerRequest {
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: user_prompt,
                },
            ],
            max_tokens: options.max_tokens,
            temperature: options.temperature,
            top_p: options.top_p,
            stream: true,
        })
        .send()
        .context("Не удалось отправить запрос локальной модели")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(anyhow!("Сервер локальной модели вернул {status}: {body}"));
    }

    read_stream(response, cancel)
}

#[derive(Serialize)]
struct ServerRequest<'a> {
    messages: Vec<ChatMessage<'a>>,
    max_tokens: u32,
    temperature: f32,
    top_p: f32,
    stream: bool,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

fn read_stream(response: reqwest::blocking::Response, cancel: Arc<CancelToken>) -> Result<String> {
    read_stream_lines(BufReader::new(response), cancel)
}

fn read_stream_lines(mut reader: impl BufRead, cancel: Arc<CancelToken>) -> Result<String> {
    let mut line = String::new();
    let mut output = String::new();
    let mut completed = false;

    loop {
        if cancel.is_cancelled() {
            anyhow::bail!("cancelled");
        }

        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(':') {
            continue;
        }
        let Some(data) = trimmed.strip_prefix("data:") else {
            continue;
        };
        if append_stream_event(data.trim(), &mut output)? {
            completed = true;
            break;
        }
    }

    if !completed {
        anyhow::bail!("Ответ локальной модели оборвался до завершения");
    }
    Ok(output.trim().to_string())
}

fn append_stream_event(data: &str, output: &mut String) -> Result<bool> {
    if data == "[DONE]" {
        return Ok(true);
    }
    let chunk: StreamChunk =
        serde_json::from_str(data).context("mlx-lm server вернул невалидный streaming JSON")?;
    for choice in chunk.choices {
        if let Some(delta) = choice.delta {
            if let Some(content) = delta.content {
                output.push_str(&content);
            }
        }
        match choice.finish_reason.as_deref() {
            Some("stop") => return Ok(true),
            Some("length") => {
                anyhow::bail!("Локальная модель достигла лимита ответа");
            }
            Some(reason) => anyhow::bail!("Локальная модель завершила ответ с причиной {reason}"),
            None => {}
        }
    }
    Ok(false)
}

fn server_client() -> Result<&'static reqwest::blocking::Client> {
    SERVER_CLIENT
        .as_ref()
        .map_err(|error| anyhow!("Не удалось создать HTTP-клиент локальной модели: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn stream_events_append_content_until_done() {
        let mut output = String::new();

        assert!(!append_stream_event(
            r###"{"choices":[{"delta":{"content":"## Резюме\n"}}]}"###,
            &mut output,
        )
        .unwrap());
        assert!(!append_stream_event(
            r#"{"choices":[{"delta":{"content":"Готово"}}]}"#,
            &mut output,
        )
        .unwrap());
        assert!(append_stream_event("[DONE]", &mut output).unwrap());
        assert_eq!(output, "## Резюме\nГотово");
    }

    #[test]
    fn stream_rejects_output_cut_off_by_token_limit() {
        let error = append_stream_event(
            r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#,
            &mut String::new(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("достигла лимита"));
    }

    #[test]
    fn stream_rejects_eof_without_completion_marker() {
        let token = CancelToken::new();
        let input = Cursor::new("data: {\"choices\":[{\"delta\":{\"content\":\"Неполный\"}}]}\n");

        let error = read_stream_lines(input, token).unwrap_err();

        assert!(error.to_string().contains("оборвался"));
    }
}
