import type { ChangeKind } from "./api";

/** 变更类型的单字母标记，与 `git status --short` 的习惯一致。 */
export function kindLabel(kind: ChangeKind): string {
  switch (kind) {
    case "added":
      return "A";
    case "modified":
      return "M";
    case "deleted":
      return "D";
    case "renamed":
      return "R";
    case "copied":
      return "C";
    case "type_changed":
      return "T";
    case "unmerged":
      return "U";
    default:
      return "?";
  }
}

/** 变更类型的中文说明。 */
export function kindText(kind: ChangeKind): string {
  switch (kind) {
    case "added":
      return "新增";
    case "modified":
      return "修改";
    case "deleted":
      return "删除";
    case "renamed":
      return "重命名";
    case "copied":
      return "复制";
    case "type_changed":
      return "类型变更";
    case "unmerged":
      return "未合并";
    default:
      return "未知";
  }
}
