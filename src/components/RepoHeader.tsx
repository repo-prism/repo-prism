import type { RepoSnapshot } from "../lib/api";

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
    </div>
  );
}
