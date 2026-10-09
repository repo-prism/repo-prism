import { useMemo } from "react";
import type { CommitInfo } from "../lib/api";
import { formatRelativeDate } from "../lib/format";
import { assignLanes, laneColor, maxLaneOf } from "../lib/graph";

const LANE_WIDTH = 16;
const ROW_HEIGHT = 48;

interface Props {
  commits: CommitInfo[];
  /** 当前选中的提交；为 null 时无选中态。 */
  selectedSha: string | null;
  onSelect: (sha: string) => void;
}

interface LaneBackground {
  id: string;
  x: number;
  color: string;
}

export function CommitGraph({ commits, selectedSha, onSelect }: Props) {
  const graph = useMemo(() => assignLanes(commits), [commits]);
  const maxLane = useMemo(() => maxLaneOf(graph), [graph]);
  const graphWidth = Math.max((maxLane + 1) * LANE_WIDTH, LANE_WIDTH * 3);

  const laneBackgrounds = useMemo<LaneBackground[]>(() => {
    const result: LaneBackground[] = [];
    for (let i = 0; i <= maxLane; i++) {
      result.push({
        id: `lane-bg-${i}`,
        x: i * LANE_WIDTH + LANE_WIDTH / 2,
        color: laneColor(i),
      });
    }
    return result;
  }, [maxLane]);

  if (commits.length === 0) {
    return (
      <div className="empty" style={{ padding: 20 }}>
        无提交历史
      </div>
    );
  }

  return (
    <div className="commit-graph">
      <div className="commit-graph-header">
        <h2>提交历史</h2>
        <span className="muted">{commits.length} 条</span>
      </div>
      <div className="commit-list">
        {graph.map((commit) => (
          <button
            type="button"
            key={commit.sha}
            className={`commit-row${selectedSha === commit.sha ? " selected" : ""}`}
            aria-pressed={selectedSha === commit.sha}
            onClick={() => onSelect(commit.sha)}
          >
            <svg
              width={graphWidth}
              height={ROW_HEIGHT}
              className="commit-svg"
              role="img"
              aria-label={`提交 ${commit.short_sha} 的图形位置`}
            >
              <title>{`提交 ${commit.short_sha}: ${commit.subject}`}</title>
              {laneBackgrounds.map((bg) => (
                <line
                  key={bg.id}
                  x1={bg.x}
                  y1={0}
                  x2={bg.x}
                  y2={ROW_HEIGHT}
                  stroke={bg.color}
                  strokeWidth={1.5}
                  opacity={0.18}
                />
              ))}
              {commit.parent_lanes.map((pl, i) => {
                const parentSha = commit.parents[i] ?? `idx-${i}`;
                const x1 = commit.lane * LANE_WIDTH + LANE_WIDTH / 2;
                const x2 = pl * LANE_WIDTH + LANE_WIDTH / 2;
                const y0 = ROW_HEIGHT / 2;
                const y1 = ROW_HEIGHT;
                if (x1 === x2) {
                  return (
                    <line
                      key={`parent-${parentSha}`}
                      x1={x1}
                      y1={y0}
                      x2={x2}
                      y2={y1}
                      stroke={laneColor(pl)}
                      strokeWidth={2}
                      opacity={0.75}
                    />
                  );
                }
                return (
                  <path
                    key={`parent-${parentSha}`}
                    d={`M ${x1} ${y0} C ${x1} ${y1 - 6}, ${x2} ${y0 + 6}, ${x2} ${y1}`}
                    stroke={laneColor(pl)}
                    strokeWidth={2}
                    fill="none"
                    opacity={0.75}
                  />
                );
              })}
              <circle
                cx={commit.lane * LANE_WIDTH + LANE_WIDTH / 2}
                cy={ROW_HEIGHT / 2}
                r={4.5}
                fill={laneColor(commit.lane)}
                stroke="#0b0f17"
                strokeWidth={1.5}
              />
            </svg>
            <div className="commit-info">
              <div className="commit-subject">
                {commit.refs.map((r) => (
                  <span key={r} className="ref-chip">
                    {r}
                  </span>
                ))}
                <span className="subject-text">{commit.subject || "(无提交信息)"}</span>
              </div>
              <div className="commit-meta">
                <code>{commit.short_sha}</code>
                <span>{commit.author_name}</span>
                <span>{formatRelativeDate(commit.author_date)}</span>
              </div>
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}
