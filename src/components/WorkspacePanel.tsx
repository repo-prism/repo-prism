import { useState } from "react";
import { getWorkspace, type WorkspaceInfo } from "../lib/api";
import { stashLabel, worktreeName, worktreeRef } from "../lib/workspace";

interface Props {
  repoPath: string;
}

/**
 * 工作树与 stash（US-1，补丁 P-09）。
 *
 * 两块**默认折叠、展开时才取**：`worktrees` 与 `stashes` 各要一次 `git` 子进程，
 * 而绝大多数仓库两者都是空的 —— 无条件取就会给「打开仓库」白添两次子进程，
 * 换来的多半是一个什么都没有的列表。
 *
 * 这里只**观测**：不提供任何创建工作树 / 应用 stash 的入口 —— 那都是写操作，
 * 与本项目的只读宪法冲突。
 *
 * 换仓库时由调用方的 `key={repoPath}` 重挂载，清掉上一个仓库的缓存结果。
 */
export function WorkspacePanel({ repoPath }: Props) {
  const [open, setOpen] = useState(false);
  const [workspace, setWorkspace] = useState<WorkspaceInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function toggle() {
    if (open) {
      setOpen(false);
      return;
    }
    setOpen(true);
    if (workspace !== null) return; // 已经取过一次就够了
    setLoading(true);
    setError(null);
    try {
      setWorkspace(await getWorkspace(repoPath));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  const trees = workspace?.worktrees ?? [];
  const stashes = workspace?.stashes ?? [];

  return (
    <div className="sidebar-section">
      <button
        type="button"
        className="workspace-toggle"
        onClick={toggle}
        aria-expanded={open}
        title="链接工作树与 stash 列表"
      >
        <span className="workspace-caret">{open ? "▾" : "▸"}</span>
        工作区
        {workspace && <span className="muted">{trees.length + stashes.length}</span>}
      </button>

      {open && loading && <div className="workspace-note">读取中…</div>}
      {open && error && <div className="workspace-note error">{error}</div>}

      {open && workspace && trees.length === 0 && stashes.length === 0 && (
        <div className="workspace-note">没有链接工作树，也没有 stash</div>
      )}

      {open && trees.length > 0 && (
        <>
          <h3>
            工作树 <span className="muted">{trees.length}</span>
          </h3>
          <ul className="ref-list">
            {trees.map((tree) => (
              <li key={tree.path}>
                <span className="ref-name" title={tree.path}>
                  {worktreeName(tree)}
                </span>
                {tree.is_main && (
                  <span className="worktree-flag" title="主工作树">
                    主
                  </span>
                )}
                {tree.locked && (
                  <span className="worktree-flag" title="已锁定">
                    锁
                  </span>
                )}
                <span className="worktree-ref">{worktreeRef(tree)}</span>
              </li>
            ))}
          </ul>
        </>
      )}

      {open && stashes.length > 0 && (
        <>
          <h3>
            stash <span className="muted">{stashes.length}</span>
          </h3>
          <ul className="ref-list">
            {stashes.map((stash) => (
              <li key={stash.reference}>
                <span className="ref-name" title={stash.message}>
                  {stashLabel(stash)}
                </span>
                <code className="ref-sha">{stash.commit.slice(0, 7)}</code>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
