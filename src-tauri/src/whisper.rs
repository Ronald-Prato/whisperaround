//! OpenAI Whisper transcription over HTTPS.

use anyhow::{anyhow, Result};
use serde::Deserialize;

const ENDPOINT: &str = "https://api.openai.com/v1/audio/transcriptions";

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: String,
}

/// Send a WAV buffer to the OpenAI transcription endpoint and return the text.
///
/// `model` is typically `whisper-1` (also accepts `gpt-4o-transcribe` /
/// `gpt-4o-mini-transcribe`).
pub async fn transcribe(api_key: &str, model: &str, wav: Vec<u8>) -> Result<String> {
    if api_key.trim().is_empty() {
        return Err(anyhow!("API key de OpenAI no configurada (ve a Ajustes)"));
    }

    let part = reqwest::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let form = reqwest::multipart::Form::new()
        .text("model", model.to_string())
        .part("file", part);

    let resp = reqwest::Client::new()
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| anyhow!("error de red: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("OpenAI respondió {status}: {body}"));
    }

    let parsed: TranscriptionResponse = resp
        .json()
        .await
        .map_err(|e| anyhow!("respuesta inválida de OpenAI: {e}"))?;
    Ok(parsed.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The API key is only ever sent here: OpenAI, over HTTPS.
    #[test]
    fn key_goes_only_to_openai_over_https() {
        assert!(ENDPOINT.starts_with("https://api.openai.com/"));
    }

    #[test]
    fn missing_key_fails_before_any_request() {
        let err = tauri::async_runtime::block_on(transcribe("  ", "whisper-1", vec![]))
            .unwrap_err();
        assert!(err.to_string().contains("API key"));
    }
}
