import { describe, expect, it } from "vitest";
import type { Risk } from "./api";
import { groupRisksByPath, riskLevelText, riskTooltip, topRisk } from "./risk";

function risk(path: string, level: Risk["level"], ruleId = "r"): Risk {
  return { rule_id: ruleId, level, message: `${level}:${path}`, path };
}

describe("topRisk", () => {
  it("按 关键 > 警告 > 提示 取最高等级", () => {
    const picked = topRisk([risk("a.rs", "info"), risk("a.rs", "critical"), risk("a.rs", "warn")]);
    expect(picked?.level).toBe("critical");
  });

  it("同等级取先出现的那条，避免反复渲染时角标跳变", () => {
    const picked = topRisk([risk("a.rs", "warn", "first"), risk("a.rs", "warn", "second")]);
    expect(picked?.rule_id).toBe("first");
  });

  it("只看等级不看数组顺序", () => {
    // 后端规则表可能调整顺序；等级更高的那条必须仍然胜出
    expect(topRisk([risk("a.rs", "warn"), risk("a.rs", "critical")])?.level).toBe("critical");
    expect(topRisk([risk("a.rs", "critical"), risk("a.rs", "warn")])?.level).toBe("critical");
  });

  it("没有风险时返回 null", () => {
    expect(topRisk([])).toBeNull();
  });
});

describe("groupRisksByPath", () => {
  it("同一路径的多条风险归到一组，且保持原顺序", () => {
    const grouped = groupRisksByPath([
      risk("a.rs", "info", "public-api"),
      risk("b.rs", "warn"),
      risk("a.rs", "critical", "env-or-secret"),
    ]);

    expect(grouped.size).toBe(2);
    expect(grouped.get("a.rs")?.map((r) => r.rule_id)).toEqual(["public-api", "env-or-secret"]);
    expect(grouped.get("b.rs")).toHaveLength(1);
  });

  it("空输入得到空表", () => {
    expect(groupRisksByPath([]).size).toBe(0);
  });
});

describe("riskTooltip", () => {
  it("把命中的全部风险拼成多行，并带上等级", () => {
    const text = riskTooltip([risk("a.rs", "critical"), risk("a.rs", "info")]);
    expect(text?.split("\n")).toHaveLength(2);
    expect(text).toContain("[关键]");
    expect(text).toContain("[提示]");
  });

  it("没有风险时不产出空 tooltip", () => {
    expect(riskTooltip([])).toBeUndefined();
  });
});

describe("riskLevelText", () => {
  it("三个等级都有中文文案", () => {
    expect(riskLevelText("info")).toBe("提示");
    expect(riskLevelText("warn")).toBe("警告");
    expect(riskLevelText("critical")).toBe("关键");
  });
});
