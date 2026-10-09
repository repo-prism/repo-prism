import { useState } from "react";
import { BranchList } from "./components/BranchList";
import { ChangesPanel } from "./components/ChangesPanel";
import { CommitDetail } from "./components/CommitDetail";
import { CommitGraph } from "./components/CommitGraph";
import { IntegrationBar } from "./components/IntegrationBar";
import { RepoHeader } from "./components/RepoHeader";
import {
  analyzeChanges,
  type ChangeAnalysis,
  type CommitDetail as CommitDetailData,
  type CommitInfo,
  type Diff,
  getCommitDetail,
  getCommitDiff,
  getCommits,
  getRemoteInfo,
  inspectRepo,
  type RemoteInfo,
  type RepoSnapshot,
} from "./lib/api";
import "./App.css";

const COMMIT_PAGE_SIZE = 300;

export default function App() {
  const [inputPath, setInputPath] = useState<string>(".");
  const [snapshot, setSnapshot] = useState<RepoSnapshot | null>(null);
  const [commits, setCommits] = useState<CommitInfo[]>([]);
  const [analysis, setAnalysis] = useState<ChangeAnalysis | null>(null);
  const [remote, setRemote] = useState<RemoteInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [selectedSha, setSelectedSha] = useState<string | null>(null);
  const [detail, setDetail] = useState<CommitDetailData | null>(null);
  const [diff, setDiff] = useState<Diff | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);

  // 详情 / Diff 一律以已解析出的仓库根为准，避免用户改动输入框后两次请求指向不同仓库
  const repoPath = snapshot?.path ?? inputPath;

  function closeDetail() {
    setSelectedSha(null);
    setDetail(null);
    setDiff(null);
    setDetailError(null);
  }

  async function load() {
    setLoading(true);
    setError(null);
    closeDetail();
    try {
      const snap = await inspectRepo(inputPath);
      setSnapshot(snap);
      // 提交历史、风险分析、远端信息三者互不依赖，且都只需已解析出的仓库根
      const [nextCommits, nextAnalysis, nextRemote] = await Promise.all([
        getCommits(snap.path, COMMIT_PAGE_SIZE, 0),
        analyzeChanges(snap.path),
        getRemoteInfo(snap.path),
      ]);
      setCommits(nextCommits);
      setAnalysis(nextAnalysis);
      setRemote(nextRemote);
    } catch (e) {
      setError(String(e));
      setSnapshot(null);
      setCommits([]);
      setAnalysis(null);
      setRemote(null);
    } finally {
      setLoading(false);
    }
  }

  async function selectCommit(sha: string) {
    setSelectedSha(sha);
    setDetailLoading(true);
    setDetailError(null);
    setDetail(null);
    setDiff(null);
    try {
      // 详情与 diff 无依赖关系，并行请求
      const [nextDetail, nextDiff] = await Promise.all([
        getCommitDetail(repoPath, sha),
        getCommitDiff(repoPath, sha),
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
              if (e.key === "Enter") load();
            }}
          />
          <button type="button" onClick={load} disabled={loading}>
            {loading ? "读取中…" : "打开"}
          </button>
        </div>
      </header>

      {error && <div className="error-banner">{error}</div>}

      {!snapshot && !error && (
        <div className="welcome">
          <h1>RepoPrism</h1>
          <p>输入一个本地 Git 仓库路径，以只读方式查看它的多种视图。</p>
          <ul>
            <li>提交图 · 分支与标签 · 变更分组与风险标记</li>
            <li>点击任意提交查看变更文件与 Diff（统一 / 并排）</li>
            <li>一键跳转 GitDiagram / GitIngest / DeepWiki / GitHub.dev</li>
          </ul>
        </div>
      )}

      {snapshot && (
        <div className="layout">
          <aside className="sidebar">
            <RepoHeader snapshot={snapshot} />
            <BranchList branches={snapshot.branches} tags={snapshot.tags} />
          </aside>
          <main className="main">
            <IntegrationBar remote={remote} />
            <ChangesPanel status={snapshot.status} analysis={analysis} />
            <div className={`main-split${selectedSha ? " has-detail" : ""}`}>
              <CommitGraph commits={commits} selectedSha={selectedSha} onSelect={selectCommit} />
              {selectedSha && (
                <CommitDetail
                  detail={detail}
                  diff={diff}
                  loading={detailLoading}
                  error={detailError}
                  onClose={closeDetail}
                  onSelectCommit={selectCommit}
                />
              )}
            </div>
          </main>
        </div>
      )}
    </div>
  );
}
