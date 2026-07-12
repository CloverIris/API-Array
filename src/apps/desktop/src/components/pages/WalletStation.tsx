"use client";

import { Badge } from "@openai/apps-sdk-ui/components/Badge";
import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Input } from "@openai/apps-sdk-ui/components/Input";
import { Select } from "@openai/apps-sdk-ui/components/Select";
import { Switch } from "@openai/apps-sdk-ui/components/Switch";
import { Textarea } from "@openai/apps-sdk-ui/components/Textarea";
import { Tooltip } from "@openai/apps-sdk-ui/components/Tooltip";
import { ApiKeys, CheckCircle, Copy, Eye, EyeOff, FileDocument, Key, Plus } from "@openai/apps-sdk-ui/components/Icon";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  createWalletAsset,
  deleteWalletAsset,
  deleteWalletAssetSecret,
  getProviderCatalog,
  getWalletAssetImpact,
  getWalletGallery,
  probeWalletAsset,
  revealWalletSecret,
  storeSecret,
  updateWalletAsset,
  upsertCustomProviderYaml,
  validateProviderYaml,
  type InspectionReport,
  type ProviderCatalogItem,
  type WalletCard,
} from "../../lib/desktop";
import { copyText, readError } from "../shared";
import { askAppDialog } from "../dialogs/AppDialog";

async function restoreClipboardFocus(timeoutMs = 2_000) {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setFocus();
  } catch {
    // The fallback below still works in a browser preview.
  }
  window.focus();
  await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  if (document.hasFocus()) {
    await new Promise((resolve) => window.setTimeout(resolve, 80));
    return;
  }
  await new Promise<void>((resolve, reject) => {
    const onFocus = () => { window.clearTimeout(timer); resolve(); };
    const timer = window.setTimeout(() => {
      window.removeEventListener("focus", onFocus);
      reject(new Error("Windows Hello 已验证，但 API ARRAY 尚未重新获得窗口焦点。请返回应用后重试。"));
    }, timeoutMs);
    window.addEventListener("focus", onFocus, { once: true });
  });
  await new Promise((resolve) => window.setTimeout(resolve, 80));
}

function maskSecret(value: string) {
  if (value.length <= 8) return "•".repeat(value.length);
  return `${value.slice(0, 4)}${"•".repeat(Math.max(8, value.length - 8))}${value.slice(-4)}`;
}

export function WalletStation({ onOpenDirect }: { onOpenDirect: (assetId: string) => void }) {
  const [catalog, setCatalog] = useState<ProviderCatalogItem[]>([]);
  const [cards, setCards] = useState<WalletCard[]>([]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<WalletCard | null>(null);
  const [dialog, setDialog] = useState(false);
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<InspectionReport | null>(null);
  const [revealed, setRevealed] = useState<string | null>(null);
  const [secretVisible, setSecretVisible] = useState(false);
  const revealTimer = useRef<number | null>(null);
  const [providerId, setProviderId] = useState("");
  const [name, setName] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [budget, setBudget] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [advanced, setAdvanced] = useState(false);
  const [yaml, setYaml] = useState("");

  const load = async () => {
    const [nextCatalog, nextCards] = await Promise.all([getProviderCatalog(), getWalletGallery()]);
    setCatalog(nextCatalog);
    setCards(nextCards);
    setProviderId((current) => current || nextCatalog[0]?.id || "");
  };
  const clearReveal = () => {
    setRevealed(null);
    setSecretVisible(false);
    if (revealTimer.current) window.clearTimeout(revealTimer.current);
  };

  useEffect(() => { void load().catch((reason) => setError(readError(reason))); }, []);
  useEffect(() => {
    const clear = () => clearReveal();
    window.addEventListener("blur", clear);
    return () => { window.removeEventListener("blur", clear); clear(); };
  }, []);

  const assets = cards.filter((card) => card.source === "asset");
  const visible = useMemo(() => assets.filter((card) => `${card.name} ${card.providerId}`.toLowerCase().includes(query.toLowerCase())), [assets, query]);
  const provider = catalog.find((item) => item.id === providerId);
  const reset = () => {
    setEditing(false); setSelected(null); clearReveal(); setName(""); setEndpoint(provider?.defaultBaseUrl ?? "");
    setApiKey(""); setBudget(""); setEnabled(true); setAdvanced(false); setYaml("");
  };
  const openAdd = () => { reset(); setDialog(true); };
  const openEdit = (asset: WalletCard) => {
    clearReveal(); setSelected(asset); setEditing(true); setProviderId(asset.providerId); setName(asset.name);
    setEndpoint(asset.endpointOverride ?? catalog.find((item) => item.id === asset.providerId)?.defaultBaseUrl ?? "");
    setBudget(asset.budgetMicros ? String(asset.budgetMicros / 1_000_000) : ""); setEnabled(asset.enabled); setApiKey(""); setDialog(true);
  };
  const save = async () => {
    if (!name.trim() || (!editing && !apiKey.trim())) { setError("名称和 API Key 不能为空。"); return; }
    setBusy(true); setError(null);
    try {
      if (advanced) {
        const validation = await validateProviderYaml(yaml);
        if (!validation.valid) throw new Error(validation.error ?? "YAML 校验失败");
        const id = name.trim().toLowerCase().replace(/[^a-z0-9_-]+/g, "-");
        const reference = `secret://workspace/${id}/api_key`;
        await storeSecret(reference, apiKey);
        await upsertCustomProviderYaml({ yaml, instanceId: id, endpointOverride: endpoint, secretFields: { api_key: reference } });
      } else if (editing && selected) {
        setCards(await updateWalletAsset({ assetId: selected.id, name, endpointOverride: endpoint, enabled, apiKey: apiKey || undefined, monthlyBudgetMicros: budget ? Number(budget) * 1_000_000 : undefined, currency: budget ? "USD" : undefined }));
      } else {
        await createWalletAsset({ providerId, name, endpointOverride: endpoint, apiKey, monthlyBudgetMicros: budget ? Number(budget) * 1_000_000 : undefined, currency: budget ? "USD" : undefined });
        await load();
      }
      setDialog(false); reset();
    } catch (reason) { setError(readError(reason)); } finally { setBusy(false); }
  };
  const inspect = async (asset: WalletCard) => {
    setBusy(true); setError(null);
    try { setSelected(asset); setReport(await probeWalletAsset(asset.id)); }
    catch (reason) { setError(readError(reason)); } finally { setBusy(false); }
  };
  const remove = async (asset: WalletCard) => {
    setBusy(true); setError(null);
    try {
      const impact = await getWalletAssetImpact(asset.id);
      if (impact.directEndpoints.length || impact.canvases.length) throw new Error(`仍被 ${impact.directEndpoints.length} 个直出端点和 ${impact.canvases.length} 个编组方案引用。`);
      const accepted = await askAppDialog({ title: `删除“${asset.name}”？`, description: "钱包资产和对应上游凭据都会删除，此操作无法撤销。", confirmLabel: "删除资产", danger: true });
      if (!accepted) return;
      setCards(await deleteWalletAsset(asset.id));
    } catch (reason) { setError(readError(reason)); } finally { setBusy(false); }
  };
  const removeKey = async (asset: WalletCard) => {
    const accepted = await askAppDialog({ title: `删除“${asset.name}”的上游 Key？`, description: "资产和引用会保留，但相关直出与编组方案将无法调用上游。", confirmLabel: "删除 Key", danger: true });
    if (!accepted) return;
    setBusy(true);
    try { clearReveal(); setCards(await deleteWalletAssetSecret(asset.id)); setError("Key 已从 Windows Credential Manager 删除。"); }
    catch (reason) { setError(readError(reason)); } finally { setBusy(false); }
  };
  const reveal = async (asset: WalletCard) => {
    setBusy(true); setError(null);
    try {
      const result = await revealWalletSecret(asset.id);
      setSelected(asset); setRevealed(result.value); setSecretVisible(true);
      revealTimer.current = window.setTimeout(clearReveal, result.expiresInMs);
    } catch (reason) { setError(readError(reason)); } finally { setBusy(false); }
  };
  const copyKey = async (asset: WalletCard) => {
    setBusy(true); setError(null);
    try {
      const result = await revealWalletSecret(asset.id);
      await restoreClipboardFocus();
      await copyText(result.value);
      setError("已通过 Windows Hello 验证并写入系统剪贴板。API ARRAY 不会自动清空或再次改写剪贴板内容。");
    } catch (reason) { setError(readError(reason)); } finally { setBusy(false); }
  };

  return <div className="wallet-station reading-page-inner">
    <header className="wallet-station-header"><div><p className="eyebrow">API WALLET</p><h1>我有哪些 API？</h1><p>钱包只管理上游资产。Key 与 SQLite 工作区分离，由 Windows Credential Manager 保存。</p></div><Button color="primary" onClick={openAdd}><Plus />添加 API</Button></header>
    {error ? <p className={error.includes("已") ? "success-message" : "error-message"} role="alert">{error}</p> : null}
    <section className="wallet-toolbar"><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索钱包 API" aria-label="搜索钱包 API" /><Badge color="secondary" variant="soft">{assets.length} 个资产</Badge></section>
    <section className="wallet-card-grid">{visible.map((asset) => <article key={asset.id} className="wallet-asset-card">
      <div className="wallet-asset-mark">{asset.name.slice(0, 1).toUpperCase()}</div><div><strong>{asset.name}</strong><small>{asset.providerId} · {asset.endpointOverride ?? "默认端点"}</small></div>
      <Badge color={asset.configured && asset.enabled ? "success" : asset.configured ? "secondary" : "warning"} variant="soft">{asset.configured ? asset.enabled ? "Key 已保护" : "已停用" : "缺少 Key"}</Badge>
      <footer><span>{asset.requestCount} 次调用 · {asset.referenceCount} 个引用</span></footer>
      {selected?.id === asset.id && revealed ? <div className="secret-reveal" role="status">
        <div className="secret-reveal-heading"><strong>上游 API Key</strong><Tooltip content={secretVisible ? "隐藏 Key" : "显示完整 Key"}><Button color="secondary" variant="ghost" size="sm" uniform aria-label={secretVisible ? "隐藏 Key" : "显示完整 Key"} onClick={() => setSecretVisible((value) => !value)}>{secretVisible ? <EyeOff /> : <Eye />}</Button></Tooltip></div>
        <code aria-label={secretVisible ? "已显示完整 Key" : "已遮罩 Key"}>{secretVisible ? revealed : maskSecret(revealed)}</code>
        <span>Windows Hello 已验证；切换页面、窗口失焦或 30 秒后会自动隐藏。</span>
      </div> : null}
      <div className="wallet-card-actions">
        <Button color="secondary" variant="ghost" size="sm" loading={busy} onClick={() => void inspect(asset)}><CheckCircle />验证</Button>
        <Button color="primary" variant="soft" size="sm" onClick={() => onOpenDirect(asset.id)}>创建直出</Button>
        {asset.configured ? <><Button color="secondary" variant="ghost" size="sm" onClick={() => void reveal(asset)}><Key />查看 Key</Button><Button color="secondary" variant="ghost" size="sm" onClick={() => void copyKey(asset)}><Copy />安全复制</Button></> : null}
        <Button color="secondary" variant="ghost" size="sm" onClick={() => openEdit(asset)}>编辑 / 替换</Button>
        {asset.configured ? <Button color="warning" variant="ghost" size="sm" onClick={() => void removeKey(asset)}>删除 Key</Button> : null}
        <Button color="danger" variant="ghost" size="sm" onClick={() => void remove(asset)}>删除资产</Button>
      </div>
    </article>)}</section>
    {!visible.length ? <div className="empty-state"><ApiKeys /><strong>钱包还没有 API</strong><p>添加 Provider、端点和 Key 后，它才会成为可验证、可直出、可编组的钱包资产。</p><Button color="primary" onClick={openAdd}>添加第一个 API</Button></div> : null}
    {report ? <section className="inspection-card"><div className="section-heading"><h2>上游验证 · {report.provider_id}</h2><Badge color={report.overall === "healthy" ? "success" : "warning"} variant="soft">{report.overall}</Badge></div><div className="finding-grid">{Object.values(report.findings).map((finding) => <article className="finding" key={finding.dimension}><strong>{finding.dimension}</strong><p>{finding.safe_summary ?? finding.status}</p></article>)}</div></section> : null}
    {dialog ? <div className="wallet-dialog" role="dialog" aria-modal="true"><div><Button className="dialog-close" color="secondary" variant="ghost" uniform aria-label="关闭" onClick={() => setDialog(false)}>×</Button><p className="eyebrow">{editing ? "EDIT ASSET" : "ADD TO WALLET"}</p><h2>{editing ? "编辑钱包 API" : "添加 API 到钱包"}</h2>{!editing ? <Button color="secondary" variant="ghost" size="sm" onClick={() => setAdvanced((value) => !value)}><FileDocument />{advanced ? "使用内置 Provider" : "使用自定义 YAML"}</Button> : null}{advanced ? <label className="field-label">Provider YAML<Textarea value={yaml} onChange={(event) => setYaml(event.target.value)} rows={8} /></label> : <label className="field-label">Provider<Select value={providerId} disabled={editing} onChange={(option) => { setProviderId(option.value); setEndpoint(catalog.find((item) => item.id === option.value)?.defaultBaseUrl ?? ""); }} options={catalog.map((item) => ({ value: item.id, label: item.name }))} /></label>}<label className="field-label">资产名称<Input value={name} onChange={(event) => setName(event.target.value)} /></label><label className="field-label">上游 Base URL<Input value={endpoint} onChange={(event) => setEndpoint(event.target.value)} /></label><label className="field-label">上游 API Key<Input type="password" autoComplete="new-password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={editing ? "留空表示不替换" : "仅写入 Windows Credential Manager"} /></label><label className="field-label">月预算（可选，USD）<Input type="number" min="0" value={budget} onChange={(event) => setBudget(event.target.value)} /></label>{editing ? <label className="switch-line"><span>启用钱包资产</span><Switch checked={enabled} onCheckedChange={setEnabled} /></label> : null}<Button color="primary" block loading={busy} onClick={() => void save()}><Key />{editing ? "保存修改" : "保存到钱包"}</Button></div></div> : null}
    <p className="wallet-security-note">Windows 11 使用系统 Hello PIN/生物识别确认查看；Windows 10 只允许替换或删除 Key。</p>
  </div>;
}
