import type { Risk, RiskLevel } from "./api";

/** 等级由轻到重。取「最该看的」那一条时按此比较，不依赖数组顺序。 */
export const RISK_RANK: Record<RiskLevel, number> = {
  info: 0,
  warn: 1,
  critical: 2,
};

/** 角标提示文案，同时用作 `title` 与无障碍标签。 */
export function riskLevelText(level: RiskLevel): string {
  switch (level) {
    case "critical":
      return "关键";
    case "warn":
      return "警告";
    default:
      return "提示";
  }
}

/** 按路径归拢风险，让变更列表按行 O(1) 取到自己的角标。 */
export function groupRisksByPath(risks: Risk[]): Map<string, Risk[]> {
  const byPath = new Map<string, Risk[]>();
  for (const risk of risks) {
    const list = byPath.get(risk.path);
    if (list) {
      list.push(risk);
    } else {
      byPath.set(risk.path, [risk]);
    }
  }
  return byPath;
}

/**
 * 取该文件最该看的一条风险：关键 > 警告 > 提示。
 *
 * 同等级下取**先出现**的那条 —— 后端规则表的顺序是稳定的，
 * 因此同一次分析反复渲染不会跳来跳去。
 */
export function topRisk(risks: Risk[]): Risk | null {
  let best: Risk | null = null;
  for (const risk of risks) {
    if (best === null || RISK_RANK[risk.level] > RISK_RANK[best.level]) {
      best = risk;
    }
  }
  return best;
}

/** 悬停提示：把该文件命中的全部风险拼成多行。 */
export function riskTooltip(risks: Risk[]): string | undefined {
  if (risks.length === 0) {
    return undefined;
  }
  return risks.map((risk) => `[${riskLevelText(risk.level)}] ${risk.message}`).join("\n");
}
