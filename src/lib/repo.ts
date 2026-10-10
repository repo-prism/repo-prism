// 多仓库工作区（US-9 / TASK-019）的纯函数。
//
// 全部与界面无关，因此可断言。仓库切换条的「显示成什么」与「关掉之后选谁」
// 这两件事都有真实边界情况（Windows 的反斜杠、路径以分隔符结尾、关掉的是
// 列表里最后一个），放在这里比散在组件里更容易守住。

/**
 * 仓库在切换条上显示的名字：路径的最后一段。
 *
 * 只做字符串处理，**不去问仓库** —— 仓库的真实名字要读 remote 或目录名才
 * 知道，那是一个子进程；而切换条只需要一个能分辨彼此的标签。
 */
export function repoDisplayName(path: string): string {
  // 去掉结尾的分隔符：`/a/b/` 的最后一段是 `b`，不是空字符串。
  const trimmed = path.replace(/[\\/]+$/, "");
  if (trimmed === "") {
    // 全是分隔符（极端情况下的根路径）：没有「最后一段」可用
    return path;
  }
  const segments = trimmed.split(/[\\/]/);
  const last = segments[segments.length - 1];
  return last === "" ? trimmed : last;
}

/**
 * 关掉 `closed` 之后该把哪个仓库变成当前仓库。
 *
 * 规则：**优先它后面那一个，其次它前面那一个** —— 这样连续关闭时焦点一直
 * 往右走，不会来回跳。关掉的是唯一一个仓库时返回 `null`（界面回到欢迎页）。
 */
export function nextActiveAfterClose(paths: string[], closed: string): string | null {
  const index = paths.indexOf(closed);
  if (index < 0) {
    return paths[0] ?? null;
  }
  const rest = paths.filter((p) => p !== closed);
  // 关掉的是最后一个时，「后面那一个」不存在，退回它前面那一个
  return rest[index] ?? rest[index - 1] ?? null;
}
