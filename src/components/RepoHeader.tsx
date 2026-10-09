import type { RepoSnapshot, UpstreamInfo } from "../lib/api";

interface Props {
  snapshot: RepoSnapshot;
}

export function RepoHeader({ snapshot }: Props) {
  const { head, path } = snapshot;
  return (
    <div className="repo-header">
      <div className="repo-path" title={path}>
        {path}
      </div>
      <div className="repo-head">
        {head.detached ? (
          <span className="branch-chip detached">detached</span>
        ) : (
          <span className="branch-chip">{head.branch}</span>
        )}
        <code>{head.commit.slice(0, 8)}</code>
      </div>
      <UpstreamLine upstream={head.upstream} />
    </div>
  );
}

/**
 * 上游跟踪状态。
 *
 * `upstream` 为 `null` 表示当前分支未设置上游——这与「与上游完全同步」是
 * 两种不同状态，因此不能都渲染成 `↑0 ↓0`。
 */
function UpstreamLine({ upstream }: { upstream: UpstreamInfo | null }) {
  if (!upstream) {
    return (
      <div className="repo-upstream muted">
        <span className="upstream-none">无上游分支</span>
      </div>
    );
  }

  const { ahead, behind } = upstream;
  const synced = ahead === 0 && behind === 0;

  return (
    <div className="repo-upstream">
      <span className="upstream-name" title={upstream.name}>
        {upstream.name}
      </span>
      {synced ? (
        <span className="upstream-count synced">与上游一致</span>
      ) : (
        <span className="upstream-count">
          {ahead > 0 && <span className="ahead">↑{ahead}</span>}
          {behind > 0 && <span className="behind">↓{behind}</span>}
        </span>
      )}
    </div>
  );
}
