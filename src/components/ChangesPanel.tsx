import type { FileChange, StatusInfo } from "../lib/api";
import { kindLabel, kindText } from "../lib/kinds";

interface Props {
  status: StatusInfo;
}

export function ChangesPanel({ status }: Props) {
  const total = status.conflicts.length + status.staged.length + status.unstaged.length;
  return (
    <div className="changes-panel">
      <div className="panel-header">
        <h2>变更</h2>
        <span className="muted">{total} 个文件</span>
      </div>
      {total === 0 && <div className="empty">工作区干净</div>}
      <ChangeGroup title="冲突" files={status.conflicts} tone="danger" />
      <ChangeGroup title="已暂存" files={status.staged} tone="success" />
      <ChangeGroup title="工作区" files={status.unstaged} tone="warning" />
    </div>
  );
}

function ChangeGroup({ title, files, tone }: { title: string; files: FileChange[]; tone: string }) {
  if (files.length === 0) return null;
  return (
    <div className={`change-group tone-${tone}`}>
      <div className="group-title">
        {title} <span className="muted">{files.length}</span>
      </div>
      <ul>
        {files.map((f) => (
          <li key={`${tone}-${f.path}`}>
            <span className={`kind kind-${f.kind}`} title={kindText(f.kind)}>
              {kindLabel(f.kind)}
            </span>
            <span className="path">{f.path}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
