import type { RemoteInfo } from "./api";

/** 一个外部工具的跳转入口。`build` 是纯函数，便于单测。 */
export interface Integration {
  id: string;
  label: string;
  description: string;
  build: (remote: RemoteInfo) => string;
}

/**
 * 外部工具清单。定位是「编排而非重造」：能跳转的绝不自己再实现一遍。
 *
 * 全部由 `owner` / `repo` 拼出，不访问网络。
 */
export const INTEGRATIONS: Integration[] = [
  {
    id: "gitdiagram",
    label: "GitDiagram",
    description: "生成项目架构图",
    build: (remote) => `https://gitdiagram.com/${remote.owner}/${remote.repo}`,
  },
  {
    id: "gitingest",
    label: "GitIngest",
    description: "仓库文本化，便于喂给 AI",
    build: (remote) => `https://gitingest.com/${remote.owner}/${remote.repo}`,
  },
  {
    id: "deepwiki",
    label: "DeepWiki",
    description: "AI 生成的仓库百科",
    build: (remote) => `https://deepwiki.com/${remote.owner}/${remote.repo}`,
  },
  {
    id: "github-dev",
    label: "GitHub.dev",
    description: "网页版 VS Code",
    build: (remote) => `https://github.dev/${remote.owner}/${remote.repo}`,
  },
  {
    id: "host",
    label: "仓库主页",
    description: "打开托管平台上的仓库首页",
    build: (remote) => `https://${remote.host}/${remote.owner}/${remote.repo}`,
  },
];
