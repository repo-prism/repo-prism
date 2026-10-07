import type { BranchInfo, TagInfo } from "../lib/api";

interface Props {
  branches: BranchInfo[];
  tags: TagInfo[];
}

export function BranchList({ branches, tags }: Props) {
  return (
    <div className="sidebar-section">
      <h3>
        分支 <span className="muted">{branches.length}</span>
      </h3>
      <ul className="ref-list">
        {branches.map((b) => (
          <li key={b.name} className={b.is_current ? "current" : ""}>
            <span className="ref-name">{b.name}</span>
            <code className="ref-sha">{b.commit.slice(0, 7)}</code>
          </li>
        ))}
      </ul>
      {tags.length > 0 && (
        <>
          <h3>
            标签 <span className="muted">{tags.length}</span>
          </h3>
          <ul className="ref-list">
            {tags.map((t) => (
              <li key={t.name}>
                <span className="ref-name">{t.name}</span>
                <code className="ref-sha">{t.commit.slice(0, 7)}</code>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
