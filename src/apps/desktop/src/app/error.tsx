"use client";

import { Alert } from "@openai/apps-sdk-ui/components/Alert";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { ArrowRotateCcw, Copy } from "@openai/apps-sdk-ui/components/Icon";
import { useEffect } from "react";
import { copyText } from "../components/shared";

export default function AppError({ error, reset }: { error: Error & { digest?: string }; reset: () => void }) {
  useEffect(() => { console.error("API ARRAY UI boundary", error); }, [error]);
  const summary = error.message || "界面遇到了无法恢复的错误。";
  return <main className="grid min-h-screen place-items-center bg-surface p-6"><div className="w-full max-w-xl"><Alert color="danger" title="主界面无法继续显示" description={summary} actionsPlacement="bottom" actions={<div className="flex flex-wrap gap-2"><Button color="primary" onClick={reset}><ArrowRotateCcw />重新加载界面</Button><Button color="secondary" variant="soft" onClick={() => void copyText(summary)}><Copy />复制错误摘要</Button></div>} /></div></main>;
}
