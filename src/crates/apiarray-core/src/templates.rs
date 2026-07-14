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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeBlock {
    pub language: TemplateLanguage,
    pub title: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveDocumentSection {
    Paragraph {
        title: String,
        body: String,
    },
    Facts {
        title: String,
        items: Vec<LiveDocumentFact>,
    },
    Steps {
        title: String,
        items: Vec<String>,
    },
    Code {
        title: String,
        block: CodeBlock,
    },
    Note {
        title: String,
        body: String,
        tone: DocumentNoteTone,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveDocumentFact {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentNoteTone {
    Info,
    Warning,
    Security,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveDocument {
    pub schema_version: u32,
    pub title: String,
    pub summary: String,
    pub language: TemplateLanguage,
    pub endpoint_status: String,
    pub sections: Vec<LiveDocumentSection>,
    pub markdown: String,
}

/// Builds a safe, structured document from an endpoint context and one of the
/// generated language templates. The resulting Markdown never contains a real
/// credential and can be exported without frontend string assembly.
pub fn generate_live_document(
    context: &TemplateContext,
    language: TemplateLanguage,
    endpoint_status: &str,
) -> Result<LiveDocument, CoreError> {
    let template = generate_templates(context)?
        .into_iter()
        .find(|item| item.language == language)
        .ok_or_else(|| CoreError::new(ErrorCode::TemplateInvalid, "未找到请求的模板语言"))?;
    let sections = vec![
        LiveDocumentSection::Paragraph { title: "用途".to_owned(), body: "通过 API ARRAY 的本地审计入口调用 OpenAI-compatible API。请求会经过鉴权、路由、故障处理与脱敏审计。".to_owned() },
        LiveDocumentSection::Facts { title: "入口信息".to_owned(), items: vec![LiveDocumentFact { label: "Base URL".to_owned(), value: context.base_url.clone() }, LiveDocumentFact { label: "公开模型".to_owned(), value: context.model.clone() }, LiveDocumentFact { label: "运行状态".to_owned(), value: endpoint_status.to_owned() }, LiveDocumentFact { label: "流式响应".to_owned(), value: if context.stream { "已启用" } else { "当前示例未启用" }.to_owned() }] },
        LiveDocumentSection::Steps { title: "调用前准备".to_owned(), items: vec![format!("将本地入口 Token 写入环境变量或安全 Secret Store；示例使用占位符 {}。", context.token_placeholder), "确认 API ARRAY 正在运行且目标入口处于启用状态。".to_owned(), "不要把本地 Token 或上游 API Key 写入源码、日志或版本库。".to_owned()] },
        LiveDocumentSection::Code { title: "快速开始".to_owned(), block: CodeBlock { language: template.language, title: template.title, code: template.code } },
        LiveDocumentSection::Note { title: "审计与隐私".to_owned(), body: "默认审计只保存模型、状态、延迟、重试、切换和 Token 用量，不保存请求正文、响应正文、Authorization Header 或 Secret。".to_owned(), tone: DocumentNoteTone::Security },
        LiveDocumentSection::Note { title: "故障排查".to_owned(), body: "遇到 401 时检查本地入口 Token；遇到 502/503 时先在 API 钱包验证上游，再检查 Canvas 编译报告和候选健康状态。".to_owned(), tone: DocumentNoteTone::Info },
    ];
    let mut document = LiveDocument {
        schema_version: 1,
        title: format!("{} · API ARRAY 活文档", context.model),
        summary: "本地、可审计的 OpenAI-compatible 调用说明。".to_owned(),
        language,
        endpoint_status: endpoint_status.to_owned(),
        sections,
        markdown: String::new(),
    };
    document.markdown = render_live_document_markdown(&document);
    Ok(document)
}

#[must_use]
pub fn render_live_document_markdown(document: &LiveDocument) -> String {
    let mut output = format!("# {}\n\n{}\n\n", document.title, document.summary);
    for section in &document.sections {
        match section {
            LiveDocumentSection::Paragraph { title, body } => {
                output.push_str(&format!("## {title}\n\n{body}\n\n"))
            }
            LiveDocumentSection::Facts { title, items } => {
                output.push_str(&format!("## {title}\n\n"));
                for item in items {
                    output.push_str(&format!("- **{}**：{}\n", item.label, item.value));
                }
                output.push('\n');
            }
            LiveDocumentSection::Steps { title, items } => {
                output.push_str(&format!("## {title}\n\n"));
                for (index, item) in items.iter().enumerate() {
                    output.push_str(&format!("{}. {item}\n", index + 1));
                }
                output.push('\n');
            }
            LiveDocumentSection::Code { title, block } => output.push_str(&format!(
                "## {title}\n\n```{}\n{}\n```\n\n",
                markdown_language(block.language),
                block.code
            )),
            LiveDocumentSection::Note { title, body, .. } => {
                output.push_str(&format!("## {title}\n\n> {body}\n\n"))
            }
        }
    }
    output
}

const fn markdown_language(language: TemplateLanguage) -> &'static str {
    match language {
        TemplateLanguage::Curl => "bash",
        TemplateLanguage::Python => "python",
        TemplateLanguage::JavascriptTypescript => "typescript",
        TemplateLanguage::Go => "go",
        TemplateLanguage::Rust => "rust",
        TemplateLanguage::Java => "java",
        TemplateLanguage::Csharp => "csharp",
        TemplateLanguage::Cpp => "cpp",
    }
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
    fn live_document_is_structured_and_markdown_is_deterministic() -> Result<(), CoreError> {
        let context = TemplateContext {
            base_url: "http://127.0.0.1:7480/direct/main/v1".to_owned(),
            model: "smart".to_owned(),
            stream: false,
            token_placeholder: "${APIARRAY_DIRECT_MAIN_TOKEN}".to_owned(),
        };
        let document = generate_live_document(&context, TemplateLanguage::Python, "运行中")?;
        assert!(
            document
                .sections
                .iter()
                .any(|section| matches!(section, LiveDocumentSection::Code { .. }))
        );
        assert_eq!(document.markdown, render_live_document_markdown(&document));
        assert!(document.markdown.contains("```python"));
        assert!(!document.markdown.to_ascii_lowercase().contains("sk-"));
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
