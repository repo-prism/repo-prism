import { useState } from "react";
import { BranchList } from "./components/BranchList";
import { ChangesPanel } from "./components/ChangesPanel";
import { CommitGraph } from "./components/CommitGraph";
import { RepoHeader } from "./components/RepoHeader";
import { type CommitInfo, getCommits, inspectRepo, type RepoSnapshot } from "./lib/api";
import "./App.css";

export default function App() {
  const [inputPath, setInputPath] = useState<string>(".");
  const [snapshot, setSnapshot] = useState<RepoSnapshot | null>(null);
  const [commits, setCommits] = useState<CommitInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    setLoading(true);
    setError(null);
    try {
      const snap = await inspectRepo(inputPath);
      setSnapshot(snap);
      const cs = await getCommits(inputPath, 300, 0);
      setCommits(cs);
    } catch (e) {
      setError(String(e));
      setSnapshot(null);
      setCommits([]);
    } finally {
      setLoading(false);
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
            <li>提交图 · 分支与标签 · 变更分组</li>
            <li>为人类与 Agent 提供同一份结构化数据</li>
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
            <ChangesPanel status={snapshot.status} />
            <CommitGraph commits={commits} />
          </main>
        </div>
      )}
    </div>
  );
}
