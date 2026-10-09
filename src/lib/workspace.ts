import type { RepoState, StashInfo, WorktreeInfo } from "./api";

/**
 * 进行中操作的短标签，用于侧边栏角标。
 *
 * 五个分支都是穷尽的 —— `RepoState` 是可辨识联合，新增一种状态时
 * TypeScript 会在 `switch` 的返回值上直接报错，不会静默漏掉一种。
 */
export function stateLabel(state: RepoState): string {
  switch (state.kind) {
    case "merge":
      return "合并中";
    case "rebase":
      return "变基中";
    case "cherry_pick":
      return "摘取中";
    case "revert":
      return "回退中";
    case "bisect":
      return "二分查找中";
  }
}

/**
 * 状态的进度文本。**只有变基有进度**，且两边都齐才显示。
 *
 * 只显示半边（例如「3/?」）比完全不显示更容易误导：读的人会以为已经知道
 * 还剩多少步。后端把读不到的进度降级为 `null` 时，这里就跟着不显示。
 */
export function stateProgress(state: RepoState): string | null {
  if (state.kind !== "rebase") return null;
  if (state.step === null || state.total === null) return null;
  return `${state.step}/${state.total}`;
}

/** 状态的 hover 说明：说清它在等什么，而不是复述标签。 */
export function stateTitle(state: RepoState): string {
  switch (state.kind) {
    case "merge":
      return "有一次合并尚未完成（通常在等冲突解决）";
    case "rebase":
      return "变基尚未完成（通常在等冲突解决）";
    case "cherry_pick":
      return "有一次摘取提交尚未完成";
    case "revert":
      return "有一次回退提交尚未完成";
    case "bisect":
      return "二分查找进行中";
  }
}

/**
 * 工作树的显示名：路径的最后一段。
 *
 * 同时按 `/` 与 `\` 切分 —— Windows 上 git 给出的是反斜杠路径，
 * 只按正斜杠切会把整条路径当名字显示。
 */
export function worktreeName(tree: WorktreeInfo): string {
  const segments = tree.path.split(/[\\/]/).filter((part) => part.length > 0);
  return segments[segments.length - 1] ?? tree.path;
}

/** 工作树检出的位置：分支名 / 分离头指针 / bare 仓库。 */
export function worktreeRef(tree: WorktreeInfo): string {
  if (tree.bare) return "bare 仓库";
  if (tree.detached) return "分离头指针";
  return tree.branch ?? "未知引用";
}

/**
 * stash 的显示名。
 *
 * 说明文字为空时退回 `stash@{n}` —— 一个空行会让列表看起来像渲染坏了，
 * 而引用名本身至少是能定位的信息。
 */
export function stashLabel(stash: StashInfo): string {
  const message = stash.message.trim();
  return message.length > 0 ? message : stash.reference;
}
