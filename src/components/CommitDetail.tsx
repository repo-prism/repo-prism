import type { CommitDetail as CommitDetailData, Diff } from "../lib/api";
import { formatRelativeDate } from "../lib/format";
import { kindLabel, kindText } from "../lib/kinds";
import { DiffView } from "./DiffView";

interface Props {
  detail: CommitDetailData | null;
  diff: Diff | null;
  loading: boolean;
  error: string | null;
  onClose: () => void;
  onSelectCommit: (sha: string) => void;
}

/** 提交详情（US-3）：元信息 + 变更文件清单 + Diff。 */
export function CommitDetail({ detail, diff, loading, error, onClose, onSelectCommit }: Props) {
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

  const { commit, files } = detail;
  const totalAdd = files.reduce((sum, f) => sum + f.additions, 0);
  const totalDel = files.reduce((sum, f) => sum + f.deletions, 0);

  return (
    <section className="commit-detail">
      <div className="detail-header">
        <div className="detail-title">
          <h2>{commit.subject || "(无提交信息)"}</h2>
          <button type="button" className="ghost" onClick={onClose}>
            关闭
          </button>
        </div>

        <div className="detail-meta">
          <code title={commit.sha}>{commit.short_sha}</code>
          <span>{commit.author_name}</span>
          <span title={commit.author_date}>{formatRelativeDate(commit.author_date)}</span>
        </div>

        {commit.body && <pre className="detail-body">{commit.body}</pre>}

        <div className="detail-parents">
          <span className="muted">父提交</span>
          {commit.parents.length === 0 ? (
            <span className="muted">无（根提交）</span>
          ) : (
            commit.parents.map((parent) => (
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
      </div>

      <div className="detail-files">
        <div className="panel-header">
          <h3>变更文件</h3>
          <span className="muted">
            {files.length} 个 · +{totalAdd} −{totalDel}
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

      {diff ? <DiffView diff={diff} /> : <div className="empty">读取 diff…</div>}
    </section>
  );
}
