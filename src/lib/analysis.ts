/**
 * `ChangeAnalysis` 的 TypeScript 侧类型与跨语言契约（补丁 P-13）。
 *
 * 风险分析是**纯本地**的：不调用任何外部 API、不访问网络。LLM 摘要层另走
 * `summarizeChanges` / `summarizeCommit`，那份没有结构化契约（就是一段文本）。
 */

import contractJson from "../../contracts/analysis.json";
import { fieldsOf, type KeyMap, parseWire, type WireShape } from "./wireContract";

const contract = contractJson as unknown as { shape: WireShape };

/** 风险等级。前端按 `info < warn < critical` 排序取最高项作角标。 */
export type RiskLevel = "info" | "warn" | "critical";

const RISK_LEVEL_MAP: KeyMap<RiskLevel> = { info: true, warn: true, critical: true };

export const RISK_LEVELS: readonly string[] = fieldsOf(RISK_LEVEL_MAP);

export interface Risk {
  /** 规则 id 稳定，前端与测试都按它断言。 */
  rule_id: string;
  level: RiskLevel;
  message: string;
  path: string;
}

export interface RiskCounts {
  info: number;
  warn: number;
  critical: number;
}

export interface ChangeAnalysis {
  summary: string;
  total_files: number;
  risks: Risk[];
  by_level: RiskCounts;
}

/** 路径 -> 字段清单。`risks` 是数组，对应的是**元素**形状。 */
export const ANALYSIS_KEYS: Record<string, readonly string[]> = {
  "": fieldsOf<ChangeAnalysis>({
    summary: true,
    total_files: true,
    risks: true,
    by_level: true,
  }),
  risks: fieldsOf<Risk>({ rule_id: true, level: true, message: true, path: true }),
  by_level: fieldsOf<RiskCounts>({ info: true, warn: true, critical: true }),
};

/** `shape 路径.字段名` -> 允许的取值。 */
export const ANALYSIS_ENUMS: Record<string, readonly string[]> = {
  "risks.level": RISK_LEVELS,
};

/** 严格校验后端返回的风险分析。 */
export function parseChangeAnalysis(value: unknown): ChangeAnalysis {
  return parseWire<ChangeAnalysis>(contract.shape, value, "ChangeAnalysis");
}
