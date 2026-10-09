import type { ChangeAnalysis, FileChange, Risk, StatusInfo } from "../lib/api";
import { kindLabel, kindText } from "../lib/kinds";
import { groupRisksByPath, riskLevelText, riskTooltip, topRisk } from "../lib/risk";

interface Props {
  status: StatusInfo;
  analysis: ChangeAnalysis | null;
}

export function ChangesPanel({ status, analysis }: Props) {
  const total = status.conflicts.length + status.staged.length + status.unstaged.length;
  const riskMap = groupRisksByPath(analysis?.risks ?? []);

  return (
    <div className="changes-panel">
      <div className="panel-header">
        <h2>变更</h2>
        <span className="muted">{total} 个文件</span>
        {analysis && analysis.by_level.critical > 0 && (
          <span className="risk-pill critical">{analysis.by_level.critical} 关键</span>
        )}
        {analysis && analysis.by_level.warn > 0 && (
          <span className="risk-pill warn">{analysis.by_level.warn} 警告</span>
        )}
      </div>
      {analysis && total > 0 && <div className="analysis-summary">{analysis.summary}</div>}
      {total === 0 && <div className="empty">工作区干净</div>}
      <ChangeGroup title="冲突" files={status.conflicts} tone="danger" riskMap={riskMap} />
      <ChangeGroup title="已暂存" files={status.staged} tone="success" riskMap={riskMap} />
      <ChangeGroup title="工作区" files={status.unstaged} tone="warning" riskMap={riskMap} />
    </div>
  );
}

function ChangeGroup({
  title,
  files,
  tone,
  riskMap,
}: {
  title: string;
  files: FileChange[];
  tone: string;
  riskMap: Map<string, Risk[]>;
}) {
  if (files.length === 0) return null;
  return (
    <div className={`change-group tone-${tone}`}>
      <div className="group-title">
        {title} <span className="muted">{files.length}</span>
      </div>
      <ul>
        {files.map((file) => {
          const risks = riskMap.get(file.path) ?? [];
          const top = topRisk(risks);
          return (
            <li key={`${tone}-${file.path}`} title={riskTooltip(risks)}>
              <span className={`kind kind-${file.kind}`} title={kindText(file.kind)}>
                {kindLabel(file.kind)}
              </span>
              {top && (
                <span
                  className={`risk-dot ${top.level}`}
                  title={`${riskLevelText(top.level)}：${top.message}`}
                />
              )}
              <span className="path">{file.path}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
