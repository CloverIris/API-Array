"use client";

import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Menu } from "@openai/apps-sdk-ui/components/Menu";
import { Tooltip } from "@openai/apps-sdk-ui/components/Tooltip";
import { ChevronDown, ChevronRight, Plus } from "@openai/apps-sdk-ui/components/Icon";
import { useState } from "react";
import { createCanvas, createFolder, createProject, deleteCanvas, deleteFolder, deleteProject, duplicateCanvas, moveCanvas, renameCanvas, renameFolder, renameProject, type ProjectTree as Model, type PublisherSnapshot } from "../../lib/desktop";
import { readError } from "../shared";

type Canvas = Model["projects"][string]["canvases"][string];
type Folder = Model["projects"][string]["folders"][string];

export function ProjectTree({ tree, publishers, selectedProjectId, selectedCanvasId, expandedProjects, expandedFolders, onTree, onOpenCanvas, onToggleProject, onToggleFolder }: { tree: Model; publishers: PublisherSnapshot[]; selectedProjectId: string | null; selectedCanvasId: string | null; expandedProjects: string[]; expandedFolders: string[]; onTree: (tree: Model) => void; onOpenCanvas: (projectId: string, canvasId: string) => void; onToggleProject: (id: string) => void; onToggleFolder: (id: string) => void }) {
  const [busy, setBusy] = useState(false); const [error, setError] = useState<string | null>(null);
  const run = async (action: () => Promise<Model>) => { setBusy(true); setError(null); try { onTree(await action()); } catch (reason) { setError(readError(reason)); } finally { setBusy(false); } };
  const lifecycle = new Map(publishers.map((item) => [item.id, item.status]));
  const addProject = async () => { const name = window.prompt("项目名称", "新项目")?.trim(); if (name) await run(() => createProject(name)); };
  return <section className="project-tree" aria-label="项目与 Canvas"><div className="project-tree-section-label"><span>Projects</span><Tooltip content="新建项目"><Button color="secondary" variant="ghost" size="sm" uniform aria-label="新建项目" disabled={busy} onClick={() => void addProject()}><Plus /></Button></Tooltip></div>
    {Object.values(tree.projects ?? {}).map((project) => { const open = expandedProjects.includes(project.id) || selectedProjectId === project.id; const unfiled = Object.values(project.canvases ?? {}).filter((item) => !item.folderId); return <div className="project-block" key={project.id}><div className="project-row"><button className="tree-expand" aria-label={`${open ? "折叠" : "展开"}${project.name}`} aria-expanded={open} onClick={() => onToggleProject(project.id)}>{open ? <ChevronDown /> : <ChevronRight />}</button><span className="project-label">{project.name}</span><TreeMenu label={`${project.name} 项目菜单`} items={[
      ["新建文件夹", async () => { const name = window.prompt("文件夹名称", "新文件夹")?.trim(); if (name) await run(() => createFolder(project.id, name)); }],
      ["新建 Canvas", async () => { const name = window.prompt("Canvas 名称", "新 Canvas")?.trim(); if (name) await run(() => createCanvas(project.id, undefined, name)); }],
      ["重命名项目", async () => { const name = window.prompt("项目名称", project.name)?.trim(); if (name) await run(() => renameProject(project.id, name)); }],
      ["删除空项目", async () => { if (window.confirm(`删除项目“${project.name}”？`)) await run(() => deleteProject(project.id)); }],
    ]} /></div>{open ? <div>{Object.values(project.folders ?? {}).map((folder) => { const key = `${project.id}:${folder.id}`; const folderOpen = expandedFolders.includes(key) || (folder.canvasIds ?? []).includes(selectedCanvasId ?? ""); return <div className="folder-block" key={folder.id}><div className="folder-row"><button className="tree-expand" aria-label={`${folderOpen ? "折叠" : "展开"}${folder.name}`} aria-expanded={folderOpen} onClick={() => onToggleFolder(key)}>{folderOpen ? <ChevronDown /> : <ChevronRight />}</button><span className="folder-label">{folder.name}</span><TreeMenu label={`${folder.name} 文件夹菜单`} items={[
        ["新建 Canvas", async () => { const name = window.prompt("Canvas 名称", "新 Canvas")?.trim(); if (name) await run(() => createCanvas(project.id, folder.id, name)); }],
        ["重命名文件夹", async () => { const name = window.prompt("文件夹名称", folder.name)?.trim(); if (name) await run(() => renameFolder(project.id, folder.id, name)); }],
        ["删除空文件夹", async () => { if (window.confirm(`删除文件夹“${folder.name}”？`)) await run(() => deleteFolder(project.id, folder.id)); }],
      ]} /></div>{folderOpen ? (folder.canvasIds ?? []).map((id) => project.canvases?.[id]).filter(Boolean).map((canvas) => <CanvasRow key={canvas.id} canvas={canvas} projectId={project.id} folders={project.folders ?? {}} selected={selectedCanvasId === canvas.id} lifecycle={canvas.publisherId ? lifecycle.get(canvas.publisherId) : undefined} onOpen={() => onOpenCanvas(project.id, canvas.id)} run={run} />) : null}</div>; })}{unfiled.length ? <div className="folder-block"><div className="folder-row"><span /><span className="folder-label">未分类</span></div>{unfiled.map((canvas) => <CanvasRow key={canvas.id} canvas={canvas} projectId={project.id} folders={project.folders ?? {}} selected={selectedCanvasId === canvas.id} lifecycle={canvas.publisherId ? lifecycle.get(canvas.publisherId) : undefined} onOpen={() => onOpenCanvas(project.id, canvas.id)} run={run} />)}</div> : null}</div> : null}</div>; })}
    {!Object.keys(tree.projects ?? {}).length ? <Button color="secondary" variant="soft" size="sm" block onClick={() => void addProject()}><Plus />创建第一个项目</Button> : null}{error ? <p className="project-tree-error" role="alert">{error}</p> : null}</section>;
}

function CanvasRow({ canvas, projectId, folders, selected, lifecycle, onOpen, run }: { canvas: Canvas; projectId: string; folders: Record<string, Folder>; selected: boolean; lifecycle?: string; onOpen: () => void; run: (action: () => Promise<Model>) => Promise<void> }) {
  const status = lifecycle === "running" ? "running" : lifecycle === "paused" ? "paused" : lifecycle === "failed" ? "failed" : "draft";
  const moves: Array<[string, () => Promise<void>]> = [["移动到未分类", () => run(() => moveCanvas(projectId, canvas.id, undefined))], ...Object.values(folders).map((folder) => [`移动到 ${folder.name}`, () => run(() => moveCanvas(projectId, canvas.id, folder.id))] as [string, () => Promise<void>])];
  return <div className={`canvas-row ${selected ? "selected" : ""}`}><span className={`canvas-status-dot status-${status}`} aria-label={`状态：${status}`} /><button className="canvas-row-name" onClick={onOpen}>{canvas.name}</button><TreeMenu label={`${canvas.name} Canvas 菜单`} items={[
    ["重命名", async () => { const name = window.prompt("Canvas 名称", canvas.name)?.trim(); if (name) await run(() => renameCanvas(projectId, canvas.id, name)); }], ["复制", () => run(() => duplicateCanvas(projectId, canvas.id))], ...moves,
    ["删除", async () => { if (window.confirm(`删除 Canvas“${canvas.name}”？钱包资产不会被删除。`)) await run(() => deleteCanvas(projectId, canvas.id)); }],
  ]} /></div>;
}

function TreeMenu({ label, items }: { label: string; items: Array<[string, () => Promise<void>]> }) { return <Menu><Menu.Trigger><Button className="tree-menu-button" color="secondary" variant="ghost" size="sm" uniform aria-label={label}>•••</Button></Menu.Trigger><Menu.Content side="right" align="start" minWidth={170}>{items.map(([text, action], index) => <Menu.Item key={`${text}:${index}`} onSelect={() => void action()}>{text}</Menu.Item>)}</Menu.Content></Menu>; }
