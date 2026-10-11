/**
 * 三个受管入口**确实**走严格解析（补丁 P-13）。
 *
 * 这条的存在理由：契约闸门只有在**运行时必经路径**上才有意义。
 * 若只写在测试里，形状清单就退化成注释，而「注释会过时」正是 P-11 那个纯
 * Rust 测试遇到的同一个问题。
 *
 * 这里把 `invoke` 换成假的，喂一条 contract 认可的样本和一条被破坏的样本，
 * 前者必须通过、后者必须抛 `WireContractError` —— 两个方向缺一不可：
 * 只测「坏的被拒」会漏掉「好的也被拒」（那样界面永远打不开），
 * 只测「好的通过」等于没测。
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import analysisContract from "../../contracts/analysis.json";
import snapshotContract from "../../contracts/snapshot.json";
import workspaceContract from "../../contracts/workspace.json";
import { analyzeChanges, getWorkspace, inspectRepo } from "./api";
import { WireContractError } from "./wireContract";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const { invoke } = await import("@tauri-apps/api/core");
const mockedInvoke = vi.mocked(invoke);

const asSamples = (json: unknown) => json as unknown as { samples: { value: unknown }[] };

/** 去掉一个对象里的一个键，模拟「后端少给了一个字段」。 */
function withoutKey(value: unknown, key: string): unknown {
  const clone = { ...(value as Record<string, unknown>) };
  delete clone[key];
  return clone;
}

describe("受管入口的严格解析", () => {
  beforeEach(() => {
    mockedInvoke.mockReset();
  });

  const cases = [
    { label: "inspectRepo", call: () => inspectRepo("/repo"), contract: snapshotContract },
    { label: "getWorkspace", call: () => getWorkspace("/repo"), contract: workspaceContract },
    { label: "analyzeChanges", call: () => analyzeChanges("/repo"), contract: analysisContract },
  ];

  for (const { label, call, contract } of cases) {
    it(`${label}：contract 认可的样本能通过`, async () => {
      const good = asSamples(contract).samples[0]?.value;
      mockedInvoke.mockResolvedValueOnce(good);
      await expect(call()).resolves.toBeDefined();
      expect(mockedInvoke).toHaveBeenCalledTimes(1);
    });

    it(`${label}：少一个字段就抛 WireContractError，而不是静默给出半成品`, async () => {
      const good = asSamples(contract).samples[0]?.value as Record<string, unknown>;
      const [first] = Object.keys(good);
      mockedInvoke.mockResolvedValueOnce(withoutKey(good, first));
      await expect(call()).rejects.toThrow(WireContractError);
    });
  }
});
