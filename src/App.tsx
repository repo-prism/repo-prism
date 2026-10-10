import { useEffect, useState } from "react";
import { AiSettingsPanel } from "./components/AiSettingsPanel";
import { BranchList } from "./components/BranchList";
import { ChangesPanel } from "./components/ChangesPanel";
import { CommitDetail } from "./components/CommitDetail";
import { CommitGraph } from "./components/CommitGraph";
import { IntegrationBar } from "./components/IntegrationBar";
import { RepoHeader } from "./components/RepoHeader";
import { RepoSwitcher } from "./components/RepoSwitcher";
import { WorkspacePanel } from "./components/WorkspacePanel";
import {
  type AiSettings,
  analyzeChanges,
  type ChangeAnalysis,
  type CommitDetail as CommitDetailData,
  type CommitInfo,
  closeRepo,
  type Diff,
  getAiSettings,
  getCommitDetail,
  getCommitDiff,
  getCommits,
  getRemoteInfo,
  inspectRepo,
  listOpenRepos,
  type RemoteInfo,
  type RepoSnapshot,
} from "./lib/api";
import { nextActiveAfterClose } from "./lib/repo";
import "./App.css";

const COMMIT_PAGE_SIZE = 300;

/** 一个仓库**已经取到**的视图数据。
 *
 * 只存加载成功的：半截数据没有意义，界面宁可显示「还没打开」。
 */
interface RepoView {
  snapshot: RepoSnapshot;
  commits: CommitInfo[];
  analysis: ChangeAnalysis;
  remote: RemoteInfo | null;
}

export default function App() {
  const [inputPath, setInputPath] = useState<string>(".");
  // 打开列表以后端为准：会话表**有上限**，被淘汰的仓库从这里消失是正确行为，
  // 而不是「界面记得、后端忘了」的不一致。
  const [openRoots, setOpenRoots] = useState<string[]>([]);
  const [views, setViews] = useState<Record<string, RepoView>>({});
  const [activePath, setActivePath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [selectedSha, setSelectedSha] = useState<string | null>(null);
  const [detail, setDetail] = useState<CommitDetailData | null>(null);
  const [diff, setDiff] = useState<Diff | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);

  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aiSettings, setAiSettings] = useState<AiSettings | null>(null);

  // 设置一挂载就读：AI 按钮的可用状态不该等到用户打开设置面板才正确。
  useEffect(() => {
    getAiSettings()
      .then(setAiSettings)
      .catch(() => setAiSettings(null));
  }, []);

  const view = activePath === null ? null : (views[activePath] ?? null);
  const aiEnabled = aiSettings?.enabled ?? false;

  function clearDetail() {
    setSelectedSha(null);
    setDetail(null);
    setDiff(null);
    setDetailLoading(false);
    setDetailError(null);
  }

  /** 读取一个仓库的四块首屏数据并记进 `views`。 */
  async function readRepo(raw: string): Promise<string> {
    const snapshot = await inspectRepo(raw);
    const root = snapshot.path;
    // 提交历史、风险分析、远端信息三者互不依赖，且都只需已解析出的仓库根
    const [commits, analysis, remote] = await Promise.all([
      getCommits(root, COMMIT_PAGE_SIZE, 0),
      analyzeChanges(root),
      getRemoteInfo(root),
    ]);
    setViews((prev) => ({ ...prev, [root]: { snapshot, commits, analysis, remote } }));
    return root;
  }

  async function openPath(raw: string) {
    setBusy(true);
    setError(null);
    clearDetail();
    try {
      const root = await readRepo(raw);
      setOpenRoots(await listOpenRepos());
      setActivePath(root);
    } catch (e) {
      // 只影响这一次打开：已经打开着的仓库不受影响
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** 重新读取当前仓库。切回已打开的仓库**不**自动重读 —— 那是用户显式要求的。 */
  async function refreshActive() {
    if (activePath === null) return;
    setBusy(true);
    setError(null);
    clearDetail();
    try {
      await readRepo(activePath);
      setOpenRoots(await listOpenRepos());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function switchTo(root: string) {
    if (root === activePath) return;
    // 换仓库即清掉上一个仓库的选中提交：它的 sha 在另一个仓库里没有意义
    clearDetail();
    setActivePath(root);
    setError(null);
  }

  async function closePath(root: string) {
    const next = nextActiveAfterClose(openRoots, root);
    await closeRepo(root);
    const remaining = await listOpenRepos();
    setOpenRoots(remaining);
    setViews((prev) => {
      const copy = { ...prev };
      delete copy[root];
      return copy;
    });
    clearDetail();
    setActivePath(next);
  }

  async function selectCommit(sha: string) {
    if (activePath === null) return;
    setSelectedSha(sha);
    setDetailLoading(true);
    setDetailError(null);
    setDetail(null);
    setDiff(null);
    try {
      // 详情与 diff 无依赖关系，并行请求
      const [nextDetail, nextDiff] = await Promise.all([
        getCommitDetail(activePath, sha),
        getCommitDiff(activePath, sha),
      ]);
      setDetail(nextDetail);
      setDiff(nextDiff);
    } catch (e) {
      setDetailError(String(e));
    } finally {
      setDetailLoading(false);
    }
  }

  return (
    <div className="app">
      <header className="app-header">
        <div className="brand">
          <span className="brand-mark">◭</span>
          <span className="brand-name">RepoPrism</span>
          <span className="brand-sub">仓库棱镜</span>
        </div>
        <div className="open-bar">
          <input
            value={inputPath}
            onChange={(e) => setInputPath(e.target.value)}
            placeholder="仓库路径，例如 /path/to/repo"
            spellCheck={false}
            onKeyDown={(e) => {
              if (e.key === "Enter") openPath(inputPath);
            }}
          />
          <button type="button" onClick={() => openPath(inputPath)} disabled={busy}>
            {busy ? "读取中…" : "打开"}
          </button>
          {activePath !== null && (
            <button
              type="button"
              className="refresh-btn"
              onClick={refreshActive}
              disabled={busy}
              title="重新读取当前仓库"
            >
              刷新
            </button>
          )}
        </div>
        <button
          type="button"
          className={`settings-btn${aiEnabled ? " active" : ""}`}
          onClick={() => setSettingsOpen(true)}
          title={aiEnabled ? "本地 AI 已启用" : "AI 设置（默认关闭）"}
          aria-label="AI 设置"
        >
          ⚙
        </button>
      </header>

      <RepoSwitcher
        paths={openRoots}
        activePath={activePath}
        onSelect={switchTo}
        onClose={closePath}
        busy={busy}
      />

      {error && <div className="error-banner">{error}</div>}

      {!view && !error && (
        <div className="welcome">
          <h1>RepoPrism</h1>
          <p>输入一个本地 Git 仓库路径，以只读方式查看它的多种视图。</p>
          <ul>
            <li>提交图 · 分支与标签 · 变更分组与风险标记</li>
            <li>点击任意提交查看变更文件与 Diff（统一 / 并排）</li>
            <li>一键跳转 GitDiagram / GitIngest / DeepWiki / GitHub.dev</li>
            <li>可选本地 AI 摘要（Ollama，仅本机地址，默认关闭）</li>
            <li>可同时打开多个仓库，用上方标签切换</li>
          </ul>
        </div>
      )}

      {view && activePath !== null && (
        <div className="layout">
          <aside className="sidebar">
            <RepoHeader snapshot={view.snapshot} />
            <BranchList branches={view.snapshot.branches} tags={view.snapshot.tags} />
            {/* key 换仓库即重挂载：清掉上一个仓库的工作树 / stash 缓存 */}
            <WorkspacePanel key={`workspace-${activePath}`} repoPath={activePath} />
          </aside>
          <main className="main">
            <IntegrationBar remote={view.remote} />
            <ChangesPanel
              key={activePath}
              repoPath={activePath}
              status={view.snapshot.status}
              analysis={view.analysis}
              aiEnabled={aiEnabled}
            />
            <div className={`main-split${selectedSha ? " has-detail" : ""}`}>
              <CommitGraph
                commits={view.commits}
                selectedSha={selectedSha}
                onSelect={selectCommit}
              />
              {selectedSha && (
                <CommitDetail
                  key={selectedSha}
                  repoPath={activePath}
                  detail={detail}
                  diff={diff}
                  loading={detailLoading}
                  error={detailError}
                  aiEnabled={aiEnabled}
                  onClose={clearDetail}
                  onSelectCommit={selectCommit}
                />
              )}
            </div>
          </main>
        </div>
      )}

      <AiSettingsPanel
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onSaved={setAiSettings}
      />
    </div>
  );
}
