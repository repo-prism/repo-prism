/** 相对时间显示。无法解析时原样返回，不抛异常。 */
export function formatRelativeDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;

  const elapsed = Date.now() - date.getTime();
  const day = 86_400_000;
  if (elapsed < day) return "今天";
  if (elapsed < 2 * day) return "昨天";
  if (elapsed < 7 * day) return `${Math.floor(elapsed / day)} 天前`;
  return date.toLocaleDateString("zh-CN");
}
