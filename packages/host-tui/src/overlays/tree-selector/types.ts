import type { SessionTreeNode } from "piko-host-runtime";

export interface GutterInfo {
  position: number;
  show: boolean;
}

export interface FlatNode {
  node: SessionTreeNode;
  indent: number;
  showConnector: boolean;
  isLast: boolean;
  gutters: GutterInfo[];
  isVirtualRootChild: boolean;
}

export type FilterMode = "default" | "no-tools" | "user-only" | "labeled-only" | "all";
