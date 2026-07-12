"use client";

import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { Code, Copy, FileDocument } from "@openai/apps-sdk-ui/components/Icon";
import { useMemo, useState } from "react";
import { saveMarkdownDocument, type LiveDocument, type TemplateLanguage } from "../../lib/desktop";

export const languageOptions: Array<{ value: TemplateLanguage; label: string }> = [
  { value: "curl", label: "cURL" }, { value: "python", label: "Python" },
  { value: "javascript_typescript", label: "JavaScript / TypeScript" }, { value: "go", label: "Go" },
  { value: "rust", label: "Rust" }, { value: "java", label: "Java" },
  { value: "csharp", label: "C#" }, { value: "cpp", label: "C++" },
];

export function LiveDocumentation({ document, language, loading = false, onLanguage, onSave }: { document: LiveDocument | null; language: TemplateLanguage; loading?: boolean; onLanguage: (language: TemplateLanguage) => void; onSave?: () => Promise<string> }) {
  const [copied, setCopied] = useState<"code" | "markdown" | null>(null);
  const code = useMemo(() => document?.sections.find((section) => section.kind === "code"), [document]);
  const copy = async (kind: "code" | "markdown") => { if (!document) return; const value = kind === "code" && code?.kind === "code" ? code.block.code : document.markdown; await navigator.clipboard.writeText(value); setCopied(kind); window.setTimeout(() => setCopied(null), 1600); };
  const save = async () => { if (!document) return; const filename = document.title.replace(/[^\p{L}\p{N}_ ·-]+/gu, "-"); const path = onSave ? await onSave() : await saveMarkdownDocument(filename, document.markdown); window.alert(`Markdown 已保存到：\n${path}`); };
  if (!document) return <section className="live-document empty-live-document"><Code /><strong>{loading ? "正在生成活文档…" : "选择端点后生成活文档"}</strong><p>文档根据当前入口、模型、鉴权和运行状态实时生成。</p></section>;
  return <article className="live-document">
    <header className="live-document-header"><div><p className="eyebrow">LIVE DOCUMENTATION</p><h2>{document.title}</h2><p>{document.summary}</p></div><Badge color={document.endpoint_status.includes("运行") ? "success" : "secondary"} variant="soft">{document.endpoint_status}</Badge></header>
    <div className="live-document-toolbar"><Select value={language} options={languageOptions} onChange={(option) => onLanguage(option.value as TemplateLanguage)} /><Button color="secondary" variant="soft" onClick={() => void copy("code")}><Copy />{copied === "code" ? "代码已复制" : "复制代码"}</Button><Button color="secondary" variant="ghost" onClick={() => void copy("markdown")}><FileDocument />{copied === "markdown" ? "Markdown 已复制" : "复制 Markdown"}</Button><Button color="secondary" variant="ghost" onClick={() => void save()}><FileDocument />保存 .md</Button></div>
    <div className="live-document-body">{document.sections.map((section, index) => {
      if (section.kind === "paragraph") return <section key={index}><h3>{section.title}</h3><p>{section.body}</p></section>;
      if (section.kind === "facts") return <section key={index}><h3>{section.title}</h3><dl>{section.items.map((item) => <div key={item.label}><dt>{item.label}</dt><dd>{item.value}</dd></div>)}</dl></section>;
      if (section.kind === "steps") return <section key={index}><h3>{section.title}</h3><ol>{section.items.map((item) => <li key={item}>{item}</li>)}</ol></section>;
      if (section.kind === "note") return <aside key={index} className={`document-note tone-${section.tone}`}><strong>{section.title}</strong><p>{section.body}</p></aside>;
      return <section key={index} className="document-code"><div><h3>{section.title}</h3><Badge color="info" variant="soft">{section.block.title}</Badge></div><pre><code>{section.block.code}</code></pre></section>;
    })}</div>
  </article>;
}
