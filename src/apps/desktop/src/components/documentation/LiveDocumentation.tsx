"use client";

import { Alert } from "@openai/apps-sdk-ui/components/Alert";
import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { EmptyMessage } from "@openai/apps-sdk-ui/components/EmptyMessage";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { Copy, Document } from "@openai/apps-sdk-ui/components/Icon";
import { useMemo, useState } from "react";
import {
  saveMarkdownDocument,
  type LiveDocument,
  type TemplateLanguage,
} from "../../lib/desktop";
import { copyText, readError } from "../shared";

export const languageOptions: Array<{ value: TemplateLanguage; label: string }> = [
  { value: "curl", label: "cURL" },
  { value: "python", label: "Python" },
  { value: "javascript_typescript", label: "JavaScript / TypeScript" },
  { value: "go", label: "Go" },
  { value: "rust", label: "Rust" },
  { value: "java", label: "Java" },
  { value: "csharp", label: "C#" },
  { value: "cpp", label: "C++" },
];

type Props = {
  document: LiveDocument | null;
  language: TemplateLanguage;
  loading?: boolean;
  onLanguage: (language: TemplateLanguage) => void;
  onSave?: () => Promise<string>;
};

export function LiveDocumentation({ document, language, loading = false, onLanguage, onSave }: Props) {
  const [message, setMessage] = useState<{ tone: "success" | "danger"; text: string } | null>(null);
  const codeSection = useMemo(
    () => document?.sections.find((section) => section.kind === "code"),
    [document],
  );

  const copy = async (kind: "code" | "markdown") => {
    if (!document) return;
    try {
      const value = kind === "code" && codeSection?.kind === "code"
        ? codeSection.block.code
        : document.markdown;
      await copyText(value);
      setMessage({ tone: "success", text: kind === "code" ? "代码已复制。" : "Markdown 已复制。" });
    } catch (reason) {
      setMessage({ tone: "danger", text: readError(reason) });
    }
  };

  const save = async () => {
    if (!document) return;
    try {
      const filename = document.title.replace(/[^\p{L}\p{N}_ -]+/gu, "-");
      const path = onSave
        ? await onSave()
        : await saveMarkdownDocument(filename, document.markdown);
      setMessage({ tone: "success", text: `文档已保存到 ${path}` });
    } catch (reason) {
      setMessage({ tone: "danger", text: readError(reason) });
    }
  };

  if (!document) {
    return (
      <div className="rounded-2xl border border-default bg-surface">
        <EmptyMessage>
          <EmptyMessage.Icon><Document /></EmptyMessage.Icon>
          <EmptyMessage.Title>{loading ? "正在生成文档…" : "选择端点后生成文档"}</EmptyMessage.Title>
          <EmptyMessage.Description>
            文档根据当前入口、模型、鉴权和运行状态实时生成。
          </EmptyMessage.Description>
        </EmptyMessage>
      </div>
    );
  }

  return (
    <article className="grid gap-5 rounded-2xl border border-default bg-surface p-5 shadow-sm">
      <header className="flex flex-col justify-between gap-4 border-b border-subtle pb-4 md:flex-row md:items-start">
        <div>
          <p className="mb-1 text-xs font-semibold tracking-[.1em] text-secondary uppercase">Documentation</p>
          <h2 className="text-xl font-semibold">{document.title}</h2>
          <p className="mt-2 text-sm text-secondary">{document.summary}</p>
        </div>
        <Badge color={document.endpoint_status.includes("运行") ? "success" : "secondary"} variant="soft">
          {document.endpoint_status}
        </Badge>
      </header>

      <div className="sticky top-0 z-10 flex flex-wrap gap-2 rounded-xl border border-subtle bg-surface p-2 shadow-sm">
        <Select
          aria-label="示例语言"
          value={language}
          options={languageOptions}
          onChange={(option) => onLanguage(option.value as TemplateLanguage)}
        />
        <Button color="secondary" variant="soft" onClick={() => void copy("code")}>
          <Copy />复制代码
        </Button>
        <Button color="secondary" variant="ghost" onClick={() => void copy("markdown")}>
          <Document />复制 Markdown
        </Button>
        <Button color="secondary" variant="ghost" onClick={() => void save()}>
          <Document />保存 .md
        </Button>
      </div>

      {message ? (
        <Alert
          color={message.tone}
          title={message.tone === "success" ? "操作完成" : "操作失败"}
          description={message.text}
          actions={<Button color="secondary" variant="ghost" size="sm" onClick={() => setMessage(null)}>关闭</Button>}
        />
      ) : null}

      <div className="grid gap-6">
        {document.sections.map((section, index) => {
          if (section.kind === "paragraph") {
            return <DocumentText key={index} title={section.title} body={section.body} />;
          }
          if (section.kind === "facts") {
            return (
              <section key={index}>
                <h3 className="mb-2 font-semibold">{section.title}</h3>
                <dl className="grid gap-2 md:grid-cols-2">
                  {section.items.map((item) => (
                    <div className="rounded-xl border border-subtle bg-surface-secondary p-3" key={item.label}>
                      <dt className="text-xs text-secondary">{item.label}</dt>
                      <dd className="mt-1 break-all font-mono text-sm">{item.value}</dd>
                    </div>
                  ))}
                </dl>
              </section>
            );
          }
          if (section.kind === "steps") {
            return (
              <section key={index}>
                <h3 className="mb-2 font-semibold">{section.title}</h3>
                <ol className="grid list-decimal gap-2 pl-5 text-sm text-secondary">
                  {section.items.map((item) => <li key={item}>{item}</li>)}
                </ol>
              </section>
            );
          }
          if (section.kind === "note") {
            return (
              <Alert
                key={index}
                color={section.tone === "security" ? "warning" : "info"}
                variant="soft"
                title={section.title}
                description={section.body}
              />
            );
          }
          return (
            <section key={index}>
              <div className="mb-2 flex items-center justify-between gap-3">
                <h3 className="font-semibold">{section.title}</h3>
                <Badge color="info" variant="soft">{section.block.title}</Badge>
              </div>
              <SafeCodeBlock language={section.block.language} code={section.block.code} />
            </section>
          );
        })}
      </div>
    </article>
  );
}

function DocumentText({ title, body }: { title: string; body: string }) {
  return (
    <section>
      <h3 className="mb-2 font-semibold">{title}</h3>
      <div className="grid gap-2 text-sm leading-6 text-secondary">
        {body.split(/\n{2,}/).map((paragraph) => <p key={paragraph}>{paragraph}</p>)}
      </div>
    </section>
  );
}

// Apps SDK UI 0.2.2 的 CodeBlock CSS 尚不能由 Next 16 Turbopack 解析。
// 这里沿用语义令牌和官方 Button，避免修改 node_modules；升级依赖后可直接替换。
function SafeCodeBlock({ language, code }: { language: string; code: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await copyText(code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };
  return (
    <div className="overflow-hidden rounded-xl border border-default bg-surface-secondary">
      <div className="flex items-center justify-between border-b border-subtle px-3 py-2">
        <span className="font-mono text-xs text-secondary">{language}</span>
        <Button color="secondary" variant="ghost" size="sm" onClick={() => void copy()}>
          <Copy />{copied ? "已复制" : "复制"}
        </Button>
      </div>
      <pre className="max-h-[520px] overflow-auto p-4 text-sm leading-6"><code>{code}</code></pre>
    </div>
  );
}
