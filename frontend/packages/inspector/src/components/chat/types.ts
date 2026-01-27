import type { UniversalItem } from "sandbox-agent";

export type TimelineEntry = {
  id: string;
  kind: "item" | "meta";
  time: string;
  item?: UniversalItem;
  deltaText?: string;
  meta?: {
    title: string;
    detail?: string;
    severity?: "info" | "error";
  };
};
