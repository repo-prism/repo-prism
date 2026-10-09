import { useState } from "react";
import { type CommitDetail as CommitDetailData, type Diff, summarizeCommit } from "../lib/api";
import { formatBytes, formatRelativeDate } from "../lib/format";
import { kindLabel, kindText } from "../lib/kinds";
import { DiffView } from "./DiffView";

interface Props {
  repoPath: string;
  detail: CommitDetailData | null;
  diff: Diff | null;
  loading: boolean;
  error: string | null;
  /** 未启用本地 AI 时按钮置灰，并给出原因提示。 */
  aiEnabled: boolean;
  onClose: () => void;
  onSelectCommit: (sha: string) => void;
}

/**
 * 提交详情（US-3）：元信息 + 变更文件清单 + Diff；另带可选的提交级 AI 分析（TASK-014）。
 *
 * 切换提交时的状态重置靠调用方的 `key={sha}` 重挂载，不在这里用 effect 手写 ——
 * 那是 React 里更容易漏掉一条分支的写法。
 */
export function CommitDetail({
  repoPath,
  detail,
  diff,
  loading,
  error,
  aiEnabled,
  onClose,
  onSelectCommit,
}: Props) {
  const [aiSummary, setAiSummary] = useState<string | null>(null);
  const [aiLoading, setAiLoading] = useState(false);
  const [aiError, setAiError] = useState<string | null>(null);

  async function runAiSummary() {
    if (!detail) return;
    setAiLoading(true);
    setAiError(null);
    try {
      setAiSummary(await summarizeCommit(repoPath, detail.info.sha));
    } catch (e) {
      setAiError(String(e));
    } finally {
      setAiLoading(false);
    }
  }

  if (error) {
    return (
      <section className="commit-detail">
        <div className="error-banner">{error}</div>
        <button type="button" className="ghost" onClick={onClose}>
          关闭
        </button>
      </section>
    );
  }

  if (loading || !detail) {
    return (
      <section className="commit-detail">
        <div className="empty">读取提交详情…</div>
      </section>
    );
  }

  const { info, files, patch, truncated } = detail;
  const totalAdd = files.reduce((sum, f) => sum + f.additions, 0);
  const totalDel = files.reduce((sum, f) => sum + f.deletions, 0);

  return (
    <section className="commit-detail">
      <div className="detail-header">
        <div className="detail-title">
          <h2>{info.subject || "(无提交信息)"}</h2>
          <button type="button" className="ghost" onClick={onClose}>
            关闭
          </button>
        </div>

        <div className="detail-meta">
          <code title={info.sha}>{info.short_sha}</code>
          <span>{info.author_name}</span>
          <span title={info.author_date}>{formatRelativeDate(info.author_date)}</span>
        </div>

        {info.body && <pre className="detail-body">{info.body}</pre>}

        <div className="detail-parents">
          <span className="muted">父提交</span>
          {info.parents.length === 0 ? (
            <span className="muted">无（根提交）</span>
          ) : (
            info.parents.map((parent) => (
              <button
                type="button"
                key={parent}
                className="link-chip"
                title={parent}
                onClick={() => onSelectCommit(parent)}
              >
                {parent.slice(0, 7)}
              </button>
            ))
          )}
        </div>

        <div className="detail-ai">
          <button
            type="button"
            className="ai-btn"
            disabled={!aiEnabled || aiLoading}
            onClick={runAiSummary}
            title={aiEnabled ? "用本地模型分析这个提交" : "请先在右上角设置中启用本地 AI"}
          >
            {aiLoading ? "分析中…" : "AI 分析此提交"}
          </button>
          {!aiEnabled && <span className="muted">本地 AI 未启用</span>}
        </div>

        {aiSummary && (
          <div className="ai-summary">
            <span className="ai-tag">AI</span>
            {aiSummary}
          </div>
        )}
        {aiError && <div className="ai-error">{aiError}</div>}
      </div>

      <div className="detail-files">
        <div className="panel-header">
          <h3>变更文件</h3>
          <span className="muted">
            {files.length} 个 · +{totalAdd} −{totalDel} · 原始 diff {formatBytes(patch.length)}
          </span>
        </div>
        {files.length === 0 ? (
          <div className="empty">本次提交没有文件变更</div>
        ) : (
          <ul className="file-stat-list">
            {files.map((file) => (
              <li key={file.path}>
                <span className={`kind kind-${file.kind}`} title={kindText(file.kind)}>
                  {kindLabel(file.kind)}
                </span>
                <span className="path">{file.path}</span>
                {file.old_path && <span className="muted">← {file.old_path}</span>}
                {file.binary && <span className="tag">二进制</span>}
                <span className="diff-stat">
                  <span className="add-count">+{file.additions}</span>
                  <span className="del-count">-{file.deletions}</span>
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      {diff ? (
        <DiffView diff={diff} byteTruncated={truncated} />
      ) : (
        <div className="empty">读取 diff…</div>
      )}
    </section>
  );
}
