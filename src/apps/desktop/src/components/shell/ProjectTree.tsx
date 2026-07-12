"use client";

import { Button } from "@openai/apps-sdk-ui/components/Button";
import { Menu } from "@openai/apps-sdk-ui/components/Menu";
import { Tooltip } from "@openai/apps-sdk-ui/components/Tooltip";
import { Branch, ChevronDown, ChevronRight, Folder, FolderOpen, Plus } from "@openai/apps-sdk-ui/components/Icon";
import { useMemo, useState } from "react";
import { createCanvas, createFolder, createProject, deleteCanvas, deleteFolder, deleteProject, duplicateCanvas, moveCanvas, renameCanvas, renameFolder, renameProject, type ProjectTree as Model, type PublisherSnapshot } from "../../lib/desktop";
import { askAppDialog } from "../dialogs/AppDialog";
import { readError } from "../shared";

type CompositionPlan = Model["projects"][string]["canvases"][string];
type FolderModel = Model["projects"][string]["folders"][string];

type Props = {
  tree: Model;
  publishers: PublisherSnapshot[];
  selectedProjectId: string | null;
  selectedCanvasId: string | null;
  expandedProjects: string[];
  expandedFolders: string[];
  onTree: (tree: Model) => void;
  onOpenCanvas: (projectId: string, canvasId: string) => void;
  onToggleProject: (id: string) => void;
  onToggleFolder: (id: string) => void;
};

export function ProjectTree(props: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const projects = useMemo(
    () => Object.values(props.tree.projects ?? {}).sort((left, right) => left.name.localeCompare(right.name, "zh-CN") || left.id.localeCompare(right.id)),
    [props.tree.projects],
  );
  const lifecycle = useMemo(() => new Map(props.publishers.map((item) => [item.id, item.status])), [props.publishers]);
  const run = async (action: () => Promise<Model>) => {
    setBusy(true);
    setError(null);
    try { props.onTree(await action()); } catch (reason) { setError(readError(reason)); } finally { setBusy(false); }
  };
  const addProject = async () => {
    const name = await askAppDialog({ title: "新建项目", inputLabel: "项目名称", defaultValue: "新项目", confirmLabel: "创建" });
    if (name) await run(() => createProject(name));
  };

  return (
    <section className="project-tree" aria-label="项目与编组方案">
      <div className="project-tree-section-label">
        <span>项目</span>
        <Tooltip content="新建项目"><Button color="secondary" variant="ghost" size="sm" uniform aria-label="新建项目" disabled={busy} onClick={() => void addProject()}><Plus /></Button></Tooltip>
      </div>
      <div role="tree" aria-label="编组项目树" className="project-tree-items">
        {projects.map((project) => {
          const projectOpen = props.expandedProjects.includes(project.id) || props.selectedProjectId === project.id;
          const folders = Object.values(project.folders ?? {}).sort((left, right) => left.name.localeCompare(right.name, "zh-CN") || left.id.localeCompare(right.id));
          const unfiled = Object.values(project.canvases ?? {}).filter((item) => !item.folderId).sort(sortPlans);
          return <div className="project-block" role="treeitem" aria-expanded={projectOpen} key={project.id}>
            <div className="project-row">
              <button className="tree-expand" aria-label={`${projectOpen ? "折叠" : "展开"}${project.name}`} onClick={() => props.onToggleProject(project.id)}>{projectOpen ? <ChevronDown /> : <ChevronRight />}</button>
              {projectOpen ? <FolderOpen className="tree-kind-icon" aria-hidden="true" /> : <Folder className="tree-kind-icon" aria-hidden="true" />}
              <span className="project-label" title={project.name}>{project.name}</span>
              <TreeMenu label={`${project.name} 项目菜单`} items={[
                ["新建文件夹", async () => promptAndRun("新建文件夹", "文件夹名称", "新文件夹", (name) => createFolder(project.id, name), run)],
                ["新建编组方案", async () => promptAndRun("新建编组方案", "方案名称", "新编组方案", (name) => createCanvas(project.id, undefined, name), run)],
                ["重命名项目", async () => promptAndRun("重命名项目", "项目名称", project.name, (name) => renameProject(project.id, name), run, "保存")],
                ["删除空项目", async () => confirmAndRun(`删除项目“${project.name}”？`, "仅允许删除没有文件夹和编组方案的空项目。", "删除项目", () => deleteProject(project.id), run)],
              ]} />
            </div>
            {projectOpen ? <div role="group" className="project-children">
              {folders.map((folder) => <FolderBranch key={folder.id} projectId={project.id} folder={folder} allFolders={project.folders ?? {}} plans={project.canvases ?? {}} selectedCanvasId={props.selectedCanvasId} expanded={props.expandedFolders.includes(`${project.id}:${folder.id}`) || (folder.canvasIds ?? []).includes(props.selectedCanvasId ?? "")} lifecycle={lifecycle} onToggle={() => props.onToggleFolder(`${project.id}:${folder.id}`)} onOpen={props.onOpenCanvas} run={run} />)}
              {unfiled.length ? <div className="folder-block virtual-folder" role="treeitem" aria-expanded="true">
                <div className="folder-row"><span className="tree-expand tree-expand-spacer" /><Folder className="tree-kind-icon" aria-hidden="true" /><span className="folder-label">未分类编组方案</span></div>
                <div role="group" className="folder-children">{unfiled.map((plan) => <PlanRow key={plan.id} plan={plan} projectId={project.id} folders={project.folders ?? {}} selected={props.selectedCanvasId === plan.id} lifecycle={plan.publisherId ? lifecycle.get(plan.publisherId) : undefined} onOpen={() => props.onOpenCanvas(project.id, plan.id)} run={run} />)}</div>
              </div> : null}
              {!folders.length && !unfiled.length ? <p className="tree-empty">这个项目还没有编组方案。</p> : null}
            </div> : null}
          </div>;
        })}
      </div>
      {!projects.length ? <Button color="secondary" variant="soft" size="sm" block onClick={() => void addProject()}><Plus />创建第一个项目</Button> : null}
      {error ? <p className="project-tree-error" role="alert">{error}</p> : null}
    </section>
  );
}

function FolderBranch({ projectId, folder, allFolders, plans, selectedCanvasId, expanded, lifecycle, onToggle, onOpen, run }: { projectId: string; folder: FolderModel; allFolders: Record<string, FolderModel>; plans: Record<string, CompositionPlan>; selectedCanvasId: string | null; expanded: boolean; lifecycle: Map<string, string>; onToggle: () => void; onOpen: (projectId: string, canvasId: string) => void; run: (action: () => Promise<Model>) => Promise<void> }) {
  const folderPlans = (folder.canvasIds ?? []).map((id) => plans[id]).filter((item): item is CompositionPlan => Boolean(item)).sort(sortPlans);
  return <div className="folder-block" role="treeitem" aria-expanded={expanded}>
    <div className="folder-row">
      <button className="tree-expand" aria-label={`${expanded ? "折叠" : "展开"}${folder.name}`} onClick={onToggle}>{expanded ? <ChevronDown /> : <ChevronRight />}</button>
      {expanded ? <FolderOpen className="tree-kind-icon" aria-hidden="true" /> : <Folder className="tree-kind-icon" aria-hidden="true" />}
      <span className="folder-label" title={folder.name}>{folder.name}</span>
      <TreeMenu label={`${folder.name} 文件夹菜单`} items={[
        ["新建编组方案", async () => promptAndRun("新建编组方案", "方案名称", "新编组方案", (name) => createCanvas(projectId, folder.id, name), run)],
        ["重命名文件夹", async () => promptAndRun("重命名文件夹", "文件夹名称", folder.name, (name) => renameFolder(projectId, folder.id, name), run, "保存")],
        ["删除空文件夹", async () => confirmAndRun(`删除文件夹“${folder.name}”？`, "仅允许删除不包含编组方案的空文件夹。", "删除文件夹", () => deleteFolder(projectId, folder.id), run)],
      ]} />
    </div>
    {expanded ? <div role="group" className="folder-children">{folderPlans.length ? folderPlans.map((plan) => <PlanRow key={plan.id} plan={plan} projectId={projectId} folders={allFolders} selected={selectedCanvasId === plan.id} lifecycle={plan.publisherId ? lifecycle.get(plan.publisherId) : undefined} onOpen={() => onOpen(projectId, plan.id)} run={run} />) : <p className="tree-empty">文件夹为空</p>}</div> : null}
  </div>;
}

function PlanRow({ plan, projectId, folders, selected, lifecycle, onOpen, run }: { plan: CompositionPlan; projectId: string; folders: Record<string, FolderModel>; selected: boolean; lifecycle?: string; onOpen: () => void; run: (action: () => Promise<Model>) => Promise<void> }) {
  const status = lifecycle === "running" ? "running" : lifecycle === "paused" ? "paused" : lifecycle === "failed" ? "failed" : "draft";
  const moves: Array<[string, () => Promise<void>]> = [["移动到未分类", () => run(() => moveCanvas(projectId, plan.id, undefined))], ...Object.values(folders).filter((folder) => folder.id !== plan.folderId).sort((left, right) => left.name.localeCompare(right.name, "zh-CN")).map((folder) => [`移动到 ${folder.name}`, () => run(() => moveCanvas(projectId, plan.id, folder.id))] as [string, () => Promise<void>])];
  return <div className={`canvas-row ${selected ? "selected" : ""}`} role="treeitem" aria-selected={selected}>
    <span className={`canvas-status-dot status-${status}`} aria-label={`状态：${status}`} />
    <Branch className="plan-icon" aria-hidden="true" />
    <button className="canvas-row-name" title={plan.name} onClick={onOpen}>{plan.name}</button>
    <TreeMenu label={`${plan.name} 编组方案菜单`} items={[
      ["重命名编组方案", async () => promptAndRun("重命名编组方案", "方案名称", plan.name, (name) => renameCanvas(projectId, plan.id, name), run, "保存")],
      ["复制编组方案", () => run(() => duplicateCanvas(projectId, plan.id))],
      ...moves,
      ["删除编组方案", async () => confirmAndRun(`删除编组方案“${plan.name}”？`, "该方案的本地出口与运行配置会删除；钱包资产保持不变。", "删除编组方案", () => deleteCanvas(projectId, plan.id), run)],
    ]} />
  </div>;
}

async function promptAndRun(title: string, label: string, defaultValue: string, action: (name: string) => Promise<Model>, run: (action: () => Promise<Model>) => Promise<void>, confirmLabel = "创建") {
  const name = await askAppDialog({ title, inputLabel: label, defaultValue, confirmLabel });
  if (name) await run(() => action(name));
}

async function confirmAndRun(title: string, description: string, confirmLabel: string, action: () => Promise<Model>, run: (action: () => Promise<Model>) => Promise<void>) {
  if (await askAppDialog({ title, description, confirmLabel, danger: true })) await run(action);
}

function sortPlans(left: CompositionPlan, right: CompositionPlan) { return left.name.localeCompare(right.name, "zh-CN") || left.id.localeCompare(right.id); }

function TreeMenu({ label, items }: { label: string; items: Array<[string, () => Promise<void>]> }) {
  return <Menu><Menu.Trigger><Button className="tree-menu-button" color="secondary" variant="ghost" size="sm" uniform aria-label={label}>•••</Button></Menu.Trigger><Menu.Content side="right" align="start" minWidth={180}>{items.map(([text, action], index) => <Menu.Item key={`${text}:${index}`} onSelect={() => void action()}>{text}</Menu.Item>)}</Menu.Content></Menu>;
}
