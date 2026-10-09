import { describe, expect, it } from "vitest";
import type { RemoteInfo } from "./api";
import { INTEGRATIONS } from "./integrations";

const github: RemoteInfo = {
  host: "github.com",
  owner: "repo-prism",
  repo: "repo-prism",
  url: "git@github.com:repo-prism/repo-prism.git",
};

const gitlabSubgroup: RemoteInfo = {
  host: "gitlab.com",
  owner: "group",
  repo: "sub/repo",
  url: "https://gitlab.com/group/sub/repo.git",
};

describe("INTEGRATIONS", () => {
  it("id 唯一，便于 React key 与设置项覆盖", () => {
    const ids = INTEGRATIONS.map((it) => it.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("每个入口都产出 https 地址", () => {
    for (const integration of INTEGRATIONS) {
      expect(integration.build(github)).toMatch(/^https:\/\/.+\/repo-prism\/repo-prism$/);
    }
  });

  it("DeepWiki / GitIngest / GitDiagram / GitHub.dev 四个专用入口都在", () => {
    const byId = Object.fromEntries(INTEGRATIONS.map((it) => [it.id, it]));
    expect(byId.gitdiagram.build(github)).toBe("https://gitdiagram.com/repo-prism/repo-prism");
    expect(byId.gitingest.build(github)).toBe("https://gitingest.com/repo-prism/repo-prism");
    expect(byId.deepwiki.build(github)).toBe("https://deepwiki.com/repo-prism/repo-prism");
    expect(byId["github-dev"].build(github)).toBe("https://github.dev/repo-prism/repo-prism");
  });

  it("仓库主页入口用 host 而不是硬编码 github.com", () => {
    // 硬编码 github.com 会把 GitLab / Gitea 上的仓库带到错误的地方
    const host = INTEGRATIONS.find((it) => it.id === "host");
    expect(host?.build(gitlabSubgroup)).toBe("https://gitlab.com/group/sub/repo");
  });

  it("GitLab 子组路径原样保留", () => {
    for (const integration of INTEGRATIONS) {
      expect(integration.build(gitlabSubgroup)).toContain("/group/sub/repo");
    }
  });
});
