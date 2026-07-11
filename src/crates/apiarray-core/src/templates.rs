use crate::{CoreError, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateContext {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub stream: bool,
    #[serde(default = "default_token_placeholder")]
    pub token_placeholder: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateLanguage {
    Curl,
    Python,
    JavascriptTypescript,
    Go,
    Rust,
    Java,
    Csharp,
    Cpp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeTemplate {
    pub language: TemplateLanguage,
    pub title: String,
    pub code: String,
}

impl TemplateContext {
    /// 校验活文档模板上下文，并拒绝首版不允许的远程地址。
    ///
    /// # Errors
    ///
    /// Base URL 不是回环 HTTP 地址、模型为空或令牌占位符看起来像真实密钥时
    /// 返回 [`ErrorCode::TemplateInvalid`]。
    pub fn validate(&self) -> Result<(), CoreError> {
        let lower = self.base_url.to_ascii_lowercase();
        let loopback = lower.starts_with("http://127.")
            || lower.starts_with("http://localhost")
            || lower.starts_with("http://[::1]");
        if !loopback {
            return Err(CoreError::new(
                ErrorCode::TemplateInvalid,
                "MVP 活文档只允许回环 HTTP Base URL",
            ));
        }
        if self.model.trim().is_empty() {
            return Err(CoreError::new(
                ErrorCode::TemplateInvalid,
                "模板模型不能为空",
            ));
        }
        if self.token_placeholder.len() > 128
            || self
                .token_placeholder
                .to_ascii_lowercase()
                .starts_with("sk-")
        {
            return Err(CoreError::new(
                ErrorCode::TemplateInvalid,
                "模板必须使用安全占位符而不是真实令牌",
            ));
        }
        Ok(())
    }
}

/// 为 OpenAI-compatible Publisher 生成八种可复制调用模板。
///
/// # Errors
///
/// 模板上下文不满足本地发布安全约束或 JSON 序列化失败时返回错误。
#[allow(clippy::too_many_lines)]
pub fn generate_templates(context: &TemplateContext) -> Result<Vec<CodeTemplate>, CoreError> {
    context.validate()?;
    let endpoint = format!(
        "{}/chat/completions",
        context.base_url.trim_end_matches('/')
    );
    let body = json!({
        "model": context.model,
        "messages": [{"role": "user", "content": "Hello from API ARRAY"}],
        "stream": context.stream,
    });
    let body_compact = serde_json::to_string(&body)?;
    let body_pretty = serde_json::to_string_pretty(&body)?;
    let token = &context.token_placeholder;
    let escaped_body = body_compact.replace('"', "\\\"");

    Ok(vec![
        CodeTemplate {
            language: TemplateLanguage::Curl,
            title: "cURL".to_owned(),
            code: format!(
                "curl {endpoint} \\\n+  -H \"Authorization: Bearer {token}\" \\\n+  -H \"Content-Type: application/json\" \\\n+  -d '{body_compact}'"
            ),
        },
        CodeTemplate {
            language: TemplateLanguage::Python,
            title: "Python".to_owned(),
            code: format!(
                r#"import requests

response = requests.post(
    "{endpoint}",
    headers={{
        "Authorization": "Bearer {token}",
        "Content-Type": "application/json",
    }},
    json={body_pretty},
    timeout=60,
)
response.raise_for_status()
print(response.json())"#
            ),
        },
        CodeTemplate {
            language: TemplateLanguage::JavascriptTypescript,
            title: "JavaScript / TypeScript".to_owned(),
            code: format!(
                r#"const response = await fetch("{endpoint}", {{
  method: "POST",
  headers: {{
    Authorization: "Bearer {token}",
    "Content-Type": "application/json",
  }},
  body: JSON.stringify({body_pretty}),
}});

if (!response.ok) throw new Error(`HTTP ${{response.status}}`);
console.log(await response.json());"#
            ),
        },
        CodeTemplate {
            language: TemplateLanguage::Go,
            title: "Go".to_owned(),
            code: format!(
                r#"package main

import (
    "bytes"
    "fmt"
    "io"
    "net/http"
)

func main() {{
    req, _ := http.NewRequest("POST", "{endpoint}", bytes.NewBufferString("{escaped_body}"))
    req.Header.Set("Authorization", "Bearer {token}")
    req.Header.Set("Content-Type", "application/json")
    response, err := http.DefaultClient.Do(req)
    if err != nil {{ panic(err) }}
    defer response.Body.Close()
    result, _ := io.ReadAll(response.Body)
    fmt.Println(string(result))
}}"#
            ),
        },
        CodeTemplate {
            language: TemplateLanguage::Rust,
            title: "Rust".to_owned(),
            code: format!(
                r#"use reqwest::Client;

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {{
    let response = Client::new()
        .post("{endpoint}")
        .bearer_auth("{token}")
        .json(&serde_json::json!({body_pretty}))
        .send()
        .await?
        .error_for_status()?;
    println!("{{}}", response.text().await?);
    Ok(())
}}"#
            ),
        },
        CodeTemplate {
            language: TemplateLanguage::Java,
            title: "Java".to_owned(),
            code: format!(
                r#"import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;

var request = HttpRequest.newBuilder(URI.create("{endpoint}"))
    .header("Authorization", "Bearer {token}")
    .header("Content-Type", "application/json")
    .POST(HttpRequest.BodyPublishers.ofString("{escaped_body}"))
    .build();
var response = HttpClient.newHttpClient()
    .send(request, HttpResponse.BodyHandlers.ofString());
System.out.println(response.body());"#
            ),
        },
        CodeTemplate {
            language: TemplateLanguage::Csharp,
            title: "C#".to_owned(),
            code: format!(
                r#"using System.Net.Http.Headers;
using System.Text;

using var client = new HttpClient();
client.DefaultRequestHeaders.Authorization =
    new AuthenticationHeaderValue("Bearer", "{token}");
using var content = new StringContent("{escaped_body}", Encoding.UTF8, "application/json");
var response = await client.PostAsync("{endpoint}", content);
response.EnsureSuccessStatusCode();
Console.WriteLine(await response.Content.ReadAsStringAsync());"#
            ),
        },
        CodeTemplate {
            language: TemplateLanguage::Cpp,
            title: "C++ (libcurl)".to_owned(),
            code: format!(
                r#"#include <curl/curl.h>

int main() {{
    CURL* curl = curl_easy_init();
    curl_slist* headers = nullptr;
    headers = curl_slist_append(headers, "Authorization: Bearer {token}");
    headers = curl_slist_append(headers, "Content-Type: application/json");
    curl_easy_setopt(curl, CURLOPT_URL, "{endpoint}");
    curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);
    curl_easy_setopt(curl, CURLOPT_POSTFIELDS, "{escaped_body}");
    const CURLcode result = curl_easy_perform(curl);
    curl_slist_free_all(headers);
    curl_easy_cleanup(curl);
    return result == CURLE_OK ? 0 : 1;
}}"#
            ),
        },
    ])
}

fn default_token_placeholder() -> String {
    "<API_ARRAY_TOKEN>".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_all_eight_languages_without_real_secret() -> Result<(), CoreError> {
        let templates = generate_templates(&TemplateContext {
            base_url: "http://127.0.0.1:6188/v1".to_owned(),
            model: "smart".to_owned(),
            stream: false,
            token_placeholder: "<API_ARRAY_TOKEN>".to_owned(),
        })?;
        assert_eq!(templates.len(), 8);
        assert!(
            templates
                .iter()
                .all(|template| template.code.contains("smart"))
        );
        assert!(
            templates
                .iter()
                .all(|template| template.code.contains("<API_ARRAY_TOKEN>"))
        );
        Ok(())
    }

    #[test]
    fn rejects_non_loopback_template_endpoint() {
        let context = TemplateContext {
            base_url: "https://public.example/v1".to_owned(),
            model: "smart".to_owned(),
            stream: false,
            token_placeholder: "<TOKEN>".to_owned(),
        };
        assert!(generate_templates(&context).is_err());
    }
}
