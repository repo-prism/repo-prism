import type { RepoState } from "../lib/api";
import { stateLabel, stateProgress, stateTitle } from "../lib/workspace";

interface Props {
  state: RepoState;
}

/**
 * 「进行中的操作」角标。
 *
 * 只在**确实有**进行中的操作时渲染 —— 调用方依据 `snapshot.state !== null`
 * 决定是否挂载。没有进行中的操作是常态，给它一个常驻的中性角标只会占地方，
 * 也会让真正需要注意的状态淹没在噪声里。
 */
export function RepoStateBadge({ state }: Props) {
  const progress = stateProgress(state);
  return (
    <span className={`repo-state state-${state.kind}`} title={stateTitle(state)}>
      {stateLabel(state)}
      {progress !== null && <span className="state-progress">{progress}</span>}
    </span>
  );
}
