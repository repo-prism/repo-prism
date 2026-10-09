import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import type { RemoteInfo } from "../lib/api";
import { INTEGRATIONS } from "../lib/integrations";

interface Props {
  remote: RemoteInfo | null;
}

/**
 * 外部工具跳转栏（US-8）。
 *
 * 没有 `origin` remote 时不给按钮，只说明原因 —— 禁用一堆点不动的按钮
 * 比直说「没有远端」更让人困惑。
 */
export function IntegrationBar({ remote }: Props) {
  const [blocked, setBlocked] = useState<string | null>(null);

  if (!remote) {
    return (
      <div className="integration-bar">
        <span className="muted">未检测到 origin remote，外部集成不可用。</span>
      </div>
    );
  }

  // `openUrl` 需要 Tauri 运行时（以及 `opener:default` 能力）。在纯 `vite dev`
  // 里会 reject —— 把地址显示出来让人手动复制，比抛一个未处理的 Promise 强。
  function launch(url: string) {
    setBlocked(null);
    openUrl(url).catch(() => setBlocked(url));
  }

  return (
    <div className="integration-bar">
      <span className="integration-label">
        {remote.owner}/{remote.repo}
      </span>
      {INTEGRATIONS.map((integration) => (
        <button
          key={integration.id}
          type="button"
          className="integration-btn"
          title={integration.description}
          onClick={() => launch(integration.build(remote))}
        >
          {integration.label}
        </button>
      ))}
      {blocked && (
        <span className="integration-fallback">
          浏览器打开失败，请手动访问：<code>{blocked}</code>
        </span>
      )}
    </div>
  );
}
