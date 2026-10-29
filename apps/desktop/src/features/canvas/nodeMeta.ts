import type { EdgeKind, NodeType } from "../../lib/ipc";

export const NODE_TYPE_LABEL: Record<NodeType, string> = {
  root: "总课题",
  research: "研究",
  experiment: "实验",
  report: "报告",
};

export const NODE_TYPE_ICON: Record<NodeType, string> = {
  root: "◎",
  research: "◇",
  experiment: "⚗",
  report: "▤",
};

export const NODE_STATE_LABEL: Record<string, string> = {
  ready: "待开始",
  running: "执行中",
  waiting_approval: "待确认",
  attention: "需关注",
  result_available: "有结果",
};

export const EDGE_KIND_LABEL: Record<EdgeKind, string> = {
  decomposition: "分解关系",
  association: "研究关联",
};
