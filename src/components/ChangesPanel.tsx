import { useState } from "react";
import {
  type ChangeAnalysis,
  type FileChange,
  type Risk,
  type StatusInfo,
  summarizeChanges,
} from "../lib/api";
import { kindLabel, kindText } from "../lib/kinds";
import { groupRisksByPath, riskLevelText, riskTooltip, topRisk } from "../lib/risk";

interface Props {
  repoPath: string;
  status: StatusInfo;
  analysis: ChangeAnalysis | null;
  /** 未启用本地 AI 时 AI 摘要按钮置灰。 */
  aiEnabled: boolean;
}

/**
 * 变更面板（US-2 + US-7）。
 *
 * 换仓库时由调用方的 `key={repoPath}` 重挂载来清掉上一条 AI 摘要，
 * 避免它看起来像是新仓库的分析结果。
 */
export function ChangesPanel({ repoPath, status, analysis, aiEnabled }: Props) {
  const [aiSummary, setAiSummary] = useState<string | null>(null);
  const [aiLoading, setAiLoading] = useState(false);
  const [aiError, setAiError] = useState<string | null>(null);

  const total = status.conflicts.length + status.staged.length + status.unstaged.length;
  const riskMap = groupRisksByPath(analysis?.risks ?? []);

  async function runAiSummary() {
    setAiLoading(true);
    setAiError(null);
    try {
      setAiSummary(await summarizeChanges(repoPath));
    } catch (e) {
      setAiError(String(e));
    } finally {
      setAiLoading(false);
    }
  }

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
        <button
          type="button"
          className="ai-btn"
          disabled={!aiEnabled || aiLoading || total === 0}
          onClick={runAiSummary}
          title={
            aiEnabled
              ? "用本地模型总结这些改动"
              : total === 0
                ? "工作区干净，没有可总结的改动"
                : "请先在右上角设置中启用本地 AI"
          }
        >
          {aiLoading ? "分析中…" : "AI 摘要"}
        </button>
      </div>
      {analysis && total > 0 && <div className="analysis-summary">{analysis.summary}</div>}
      {aiSummary && (
        <div className="ai-summary">
          <span className="ai-tag">AI</span>
          {aiSummary}
        </div>
      )}
      {aiError && <div className="ai-error">{aiError}</div>}
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
