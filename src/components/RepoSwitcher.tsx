import { repoDisplayName } from "../lib/repo";

interface RepoSwitcherProps {
  /** 当前打开的仓库根（后端 `list_open_repos` 的结果，已排序）。 */
  paths: string[];
  activePath: string | null;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
  /** 正在读取的仓库根。禁用它的 tab，避免重复点开同一个仓库。 */
  busy: boolean;
}

/**
 * 仓库切换条（US-9 / TASK-019）。
 *
 * 显示名只取路径最后一段（`repoDisplayName`）：仓库的「真名」要读 remote 才知道，
 * 那是一个子进程，而这里只需要一个能互相分辨的标签。完整路径放在 `title` 里。
 */
export function RepoSwitcher({ paths, activePath, onSelect, onClose, busy }: RepoSwitcherProps) {
  if (paths.length === 0) {
    return null;
  }

  return (
    <nav className="repo-tabs" aria-label="已打开的仓库">
      {paths.map((path) => {
        const active = path === activePath;
        return (
          <div key={path} className={`repo-tab${active ? " active" : ""}`}>
            <button
              type="button"
              className="repo-tab-select"
              onClick={() => onSelect(path)}
              disabled={busy}
              title={path}
              aria-current={active ? "page" : undefined}
            >
              <span className="repo-tab-name">{repoDisplayName(path)}</span>
            </button>
            <button
              type="button"
              className="repo-tab-close"
              onClick={() => onClose(path)}
              title="关闭这个仓库"
              aria-label={`关闭 ${repoDisplayName(path)}`}
            >
              ×
            </button>
          </div>
        );
      })}
    </nav>
  );
}
